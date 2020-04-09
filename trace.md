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


### 可通知的Waker

`BTree<TaskId, Task>`
