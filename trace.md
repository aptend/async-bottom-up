Source: [**write an os in rust: async and await**](https://github.com/rustcc/writing-an-os-in-rust/blob/master/12-async-await.md)

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

#### crossbeam::queue

`crossbeam::queue`，线程安全的队列，每个修改内部状态的函数都可以通过`&self`执行。和`channel`的阻塞不同，边缘事件直接返回错误:

- `ArrayQueue` 列表实现，固定大小
- `SegQueue` 链表实现，不固定大小

`Blog-OS`不使用`SegQueue`，因为异步通知的动作由中断处理程序执行。中断程序应尽可能保持简练，
避免互斥锁。因为在处理过程中再触发中断就可能死锁。而`Blog-OS`的内存分配器本身使用互斥锁

> ❓ 那async_std中的future应该使用的是epoll之类的，可以使用`SegQueue`吗？

#### SimpleExecutor -> Executor

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

#### TaskWaker

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

#### 安全地构造Waker

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

#### crossbeam::channel
作为`std::sync::mpsc`的替代，可以用作`mpmc`。本来之前标准库中有`mpmc`，但是也去掉了，推荐使用`crossbeam`

就只有两种选择，是否需要容量限制:
- `bounded`
- `unbounded`

但再想这个具体的场景，可能存在很多task的Waker，在某一时间可能都要给executor发消息激活一下，理想状况下，应该是一个`bounded(1)`的有界`channel`，但是发送方不阻塞(`try_send`)，满了就跳过。此时executor被唤醒，执行下一次`[wake_tasks -> run_ready_tasks]`的循环。有可能在`wake_tasks`时出现新的可用任务，`channel`当然被填上，但是新任务也被`wake_tasks`取走，等`run_ready_tasks`完成，`sleep_if_idle`时检查到新任务为空，但是`channel`中存在item，因为无法确定这个item到底是什么时候产生的，`wake_tasks`还是`run_ready_tasks`，只好再尝试循环一遍。

> 💡又搜到这个文章，用的却是park/unpark，之后对比看 [构建你自己的block_on](https://colobu.com/2020/01/30/build-your-own-block-on/)


### Bench

参考提到的文章，先做了一个的呆瓜block_on

```rust
pub fn block_on<F: Future<Output=()> + 'static>(f: F) {
    let mut exec = Executor::new();
    exec.spawn(Task::new(f));
    exec.run();
}
```

```shell
test custom_block_on_0_yields   ... bench:       1,024 ns/iter (+/- 178)
test custom_block_on_10_yields  ... bench:       2,559 ns/iter (+/- 446)
test custom_block_on_50_yields  ... bench:       8,094 ns/iter (+/- 941)
test futures_block_on_0_yields  ... bench:          17 ns/iter (+/- 10)
test futures_block_on_10_yields ... bench:         211 ns/iter (+/- 14)
test futures_block_on_50_yields ... bench:       1,093 ns/iter (+/- 163))
```

大概是8倍的差距

tomorrow TODO:

- 用criterion做一下测试看
- 对block_on而言，`waiting_tasks`这种东西不需要，重写一下，看基于park/unpark和channel的区别

这是用`criterion`做的，差不多
```shell
my_block_on/0           time:   [1.0137 us 1.0232 us 1.0342 us]
futures_block_on/0      time:   [9.7263 ns 9.7882 ns 9.8564 ns]

my_block_on/10          time:   [2.5477 us 2.5659 us 2.5855 us]
futures_block_on/10     time:   [214.61 ns 216.91 ns 220.16 ns]

my_block_on/50          time:   [8.1675 us 8.2860 us 8.4119 us]
futures_block_on/50     time:   [1.1682 us 1.2561 us 1.3533 us]
```


把block_on换成这样，单纯使用`bounded(1)`来唤醒和睡眠
```rust
pub fn block_on<F: Future<Output=()> + 'static>(f: F) {
    let (s, r) = bounded(1);
    let mut task = Task::new(f);
    let waker = Waker::from(Arc::new(BlockWaker {
        wake_chan: s
    }));
    let mut context = Context::from_waker(&waker);
    loop {
         match task.poll(&mut context) {
            Poll::Pending => {
                r.recv().expect("can't recv from wake_chan");
            }
            Poll::Ready(()) => {
                break;
            }
        }
    }
}
```

测试结果为
```shell
my_block_on/0           time:   [455.37 ns 460.43 ns 465.97 ns]
futures_block_on/0      time:   [10.334 ns 10.388 ns 10.452 ns]

my_block_on/10          time:   [796.13 ns 802.44 ns 809.36 ns]
futures_block_on/10     time:   [210.88 ns 211.63 ns 212.46 ns]

my_block_on/50          time:   [2.1114 us 2.1537 us 2.2059 us]
futures_block_on/50     time:   [1.0507 us 1.0558 us 1.0625 us]
```

`/0`实际上没有用到Waker，所以`my_block_on`光是初始化的时间就是`futures`的40倍  
`/50`分摊初始化的时间后，性能是2~3倍，所以`bounded(1)`的初始化和效率还是低于`park/unpark`方案的
