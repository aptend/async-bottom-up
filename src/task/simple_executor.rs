use super::Task;

use std::collections::VecDeque;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

fn dummy_raw_waker() -> RawWaker {
    fn clone(_: *const ()) -> RawWaker {
        dummy_raw_waker()
    }

    fn no_op(_: *const ()) {}

    let vtable = &RawWakerVTable::new(clone, no_op, no_op, no_op);
    // vtable中的函数都是no_op, 没有操作任何data, 所以传入空指针
    RawWaker::new(0 as *const (), vtable)
}

fn dummy_waker() -> Waker {
    unsafe { Waker::from_raw(dummy_raw_waker()) }
}

pub struct SimpleExecutor {
    task_queue: VecDeque<Task>,
}

impl SimpleExecutor {
    pub fn new() -> SimpleExecutor {
        SimpleExecutor {
            task_queue: VecDeque::new(),
        }
    }

    pub fn spawn(&mut self, task: Task) {
        self.task_queue.push_back(task);
    }

    pub fn run(&mut self) {
        while let Some(mut task) = self.task_queue.pop_front() {
            let waker = dummy_waker();
            let mut context = Context::from_waker(&waker);
            match task.poll(&mut context) {
                Poll::Pending => self.task_queue.push_back(task),
                Poll::Ready(()) => {}
            }
        }
    }
}
