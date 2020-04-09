
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker, Wake};

use crossbeam::queue::ArrayQueue;

use super::{Task, TaskId};

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

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task();
    }
}

pub struct Executor {
    task_queue: VecDeque<Task>,
    waiting_tasks: BTreeMap<TaskId, Task>,
    wake_queue: Arc<ArrayQueue<TaskId>>,
    waker_cache: BTreeMap<TaskId, Waker>,
}

impl Executor {
    pub fn new() -> Executor {
        Executor {
            task_queue: VecDeque::new(),
            waiting_tasks: BTreeMap::new(),
            wake_queue: Arc::new(ArrayQueue::new(100)),
            waker_cache: BTreeMap::new(),
        }
    }

    pub fn spawn(&mut self, task: Task) {
        // 一旦开始执行后，不能再执行spawn，可以改用channel或者Arc<SegQueue>
        self.task_queue.push_back(task);
    }

    /// 从task_queue中取任务，尝试从cache中拿Waker，依次poll
    /// 如果pending就加入waiting_tasks
    fn run_ready_tasks(&mut self) {
        while let Some(mut task) = self.task_queue.pop_front() {
            let id = task.id();
            if !self.waker_cache.contains_key(&id) {
                self.waker_cache.insert(id, self.create_waker(id));
            }

            let waker = self.waker_cache.get(&id).expect("waker should exist");
            let mut context = Context::from_waker(&waker);
            match task.poll(&mut context) {
                Poll::Pending => {
                    if self.waiting_tasks.insert(id, task).is_some() {
                        panic!("task with same ID already in waiting_tasks");
                    }
                }
                Poll::Ready(()) => {
                    self.waker_cache.remove(&id);
                }
            }
        }
    }

    fn create_waker(&self, task_id: TaskId) -> Waker {
        Waker::from(Arc::new(TaskWaker {
            task_id,
            wake_queue: self.wake_queue.clone()
        }))
    }

    /// 把task_queue中的任务执行完后，从wake_queue中拿出准备好的任务
    fn wake_tasks(&mut self) {
        while let Ok(task_id) = self.wake_queue.pop() {
            if let Some(task) = self.waiting_tasks.remove(&task_id) {
                self.task_queue.push_back(task);
            }
        }
    }

    pub fn run(&mut self) {
        use std::time;
        use std::thread;
        let secs = time::Duration::from_secs(4);
        loop {
            self.wake_tasks();
            self.run_ready_tasks();
            thread::sleep(secs);
        }
    }
}
