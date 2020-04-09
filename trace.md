## Waker

### 创建
```
     fn create_raw() -> RawWaker
            |
            v
   unsafe { Waker::from_raw(creat_raw()) } -> Waker
            |
            v
     Context::from_waker(&waker) -> Context
```

### RawWaker

[`RawWakerVTable`](https://doc.rust-lang.org/std/task/struct.RawWakerVTable.html#methods) -> 虚函数表布局 -> `RawWaker`在(克隆|唤醒|删除)时的调用函数 -> 函数操作`data: *const ()`

```rust
pub struct RawWaker {
    data: *const (),
    vtable: &'static RawWakerVTable,
}
```


创建`RawWaker`的下列代码未报错
```rust
let vtable = &RawWakerVTable::new(clone, no_op, no_op, no_op);
// no-op不操作data，直接传空指针
RawWaker::new(0 as *const (), vtable)
```
因为`RawWakerVTable::new`的签名是
```rust
#[rustc_allow_const_fn_ptr]
pub const fn new(clone, wake, wake_by_ref, drop) -> RawWakerVTable
```
函数返回const，所以vtable是`'static`

> ❓ 需要深入了解吗

### 可通知的Waker


`crossbeam::queue`，线程安全的队列，每个修改内部状态的函数都可以通过`&self`执行。和`channel`的阻塞不同，边缘事件直接返回错误:

- `ArrayQueue` 列表实现，固定大小
- `SegQueue` 链表实现，不固定大小

`Blog-OS`不使用`SegQueue`，因为异步通知的动作由中断处理程序执行。中断程序应尽可能保持简练，
避免互斥锁。因为在处理过程中再触发中断就可能死锁。而`Blog-OS`的内存分配器本身使用互斥锁

> ❓ 那async_std中的future应该使用的是epoll之类的，可以使用`SegQueue`吗？

SimpleExecutor -> Executor

```rust
pub struct Executor {
    // 需要poll的task
    task_queue: VecDeque<Task>,

    // 等待被唤醒的task
    waiting_tasks: BTreeMap<TaskId, Task>,

    // 被executor和reactor(异步唤醒者)共有，
    // reactor添加，executor从中拿取，添加到task_queue
    wake_queue: Arc<ArrayQueue<TaskId>>,

    // 1.缓存Waker，每个任务可能要多次poll，不必每次都新建Waker(可行性取决于Waker的实现)
    // 2.避免死锁
    waker_cache: BTreeMap<TaskId, Waker>,
}
```

TaskWaker

`reactor`显然应该通过`Waker.wake`能向`wake_queue`添加准备好执行的任务id

```rust
struct TaskWaker {
    task_id: TaskId,
    wake_queue: Arc<ArrayQueue<TaskId>>,
}

impl TaskWaker {
    fn wake_task(&self) {
        println!("  wake task {:?}", self.task_id);
        self.wake_queue.push(self.task_id).expect("wake_queue full");
    }
}
```

安全地构造Waker

在1.44-nightly中才会有`std::task::Wake` Trait。在`futures-0.3.4`中存在相似的`futures::ArcWake`。

`std::task::Wake`要求的是`wake(self: Arc<Self>)`，消耗一个共享所有权执行唤醒。而
`wake_by_ref` 的本意是直接拿着`Arc`的引用唤醒，可以避免一次引用计数的改变。但是有的情况(?)下不支持，所以默认提供的实现是clone一次，和`wake`一样。

>❓`ArcWake`要求的却是`wake_by_ref(self: &Arc<Self>)`

不过两者在转换上到都是相似的:

- `data`是`Arc::into_raw(waker) as *const()`
- 虚函数中重新转换为具体类型后执行trait上的方法
    ```rust
    let waker: Arc<W> = Arc::from_raw(data as *const W);`
    <W as Wake>::wake(waker);
    ```
> ❓其他内存上的细节看不懂。


### 空闲时休眠

#### 游戏
```rust
fn run(&mut self) {
    let secs = time::Duration::from_secs(4);
    loop {
        self.wake_tasks();
        self.run_ready_tasks();
        thread::sleep(secs);
    }
}
```
会出现这样的输出，非常好玩
```shell
async_number: 42
hi
  wake task TaskId(2304206415664)
hi

yo
  wake task TaskId(2304206415664)
yo
```

#### 竞态条件
处理即时性，`Blog-OS`使用的是`hlt`停机直到下次中断到来。"检查是否为空-执行hlt"会出现一个微妙的竞态条件，中断是完全异步的，如果在检查为空之后，hlt执行前出现新事件，依然会执行hlt，从而推迟中断的处理，要等到下一个中断到来。

这和CSAPP中信号一节，提到的SIGCHILD类似: 子进程在父进程写入名单之前就退出，会导致父进程先删除名单，然后再添加名单。

**解决办法也都相同，屏蔽信号/中断，推进逻辑到安全点，统一打开**

**这里还可以学习到一个`fast_path`的技巧，和写单例时的双重检查类似，尽可能避免昂贵的操作**
```rust
fn sleep_if_idle(&self) {
    // fast path
    if !self.wake_queue.is_empty() {
        return;
    }

    interrupts::disable();
    if self.wake_queue.is_empty() {
        enable_interrupts_and_hlt();
    } else {
        interrupts::enable();
    }
}
```

#### park/unpark
具体到当前的标准库实现，知道这是一个典型的事件通知场景，可以用`(Condvar,Mutex)`，可以用`channel`。之前有印象`park/unpark`，看了源码还是使用的是`(Condvar,Mutex)`

```rust
/// The internal representation of a `Thread` handle
struct Inner {
    name: Option<CString>,
    id: ThreadId,
    // state for thread park/unpark
    state: AtomicUsize,
    lock: Mutex<()>,
    cvar: Condvar,
}
```

本来文档说`park`可能会无缘无故地醒来，但实现因为用了条件变量和互斥锁，并且还会检查原子变量，无缘无故醒来的情况实际上被内部处理，只是之后可能会换更有效率的实现，所以文档就没有改

em，既然在条件变量和channel里面选，那自然还是channel好用
