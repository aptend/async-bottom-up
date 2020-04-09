use std::boxed::Box;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub mod executor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TaskId(usize);

pub struct Task {
    future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + 'static) -> Task {
        Task {
            future: Box::pin(future),
        }
    }
    pub fn poll(&mut self, ctx: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(ctx)
    }

    fn id(&self) -> TaskId {
        // alternative way is:
        // use std::ops::Deref;
        // let addr = Pin::deref(&self.future) as *const _ as *const () as usize;
        let addr = &*self.future as *const _ as *const () as usize;
        TaskId(addr)
    }
}
