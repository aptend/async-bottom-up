
use criterion::{Criterion, BenchmarkId, criterion_group, criterion_main};

use futures;

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, Waker};

use async_bottom_up::task::executor;

// use std::cell::RefCell;
// use crossbeam::sync::Parker;

// /// Runs a future to completion on the current thread.
// fn block_on<F: Future>(future: F) -> F::Output {
//     // Pin the future on the stack.
//     pin_utils::pin_mut!(future);

//     thread_local! {
//         // Parker and waker associated with the current thread.
//         static CACHE: RefCell<(Parker, Waker)> = {
//             let parker = Parker::new();
//             let unparker = parker.unparker().clone();
//             let waker = async_task::waker_fn(move || unparker.unpark());
//             RefCell::new((parker, waker))
//         };
//     }

//     CACHE.with(|cache| {
//         // Panic if `block_on()` is called recursively.
//         let (parker, waker) = &mut *cache.try_borrow_mut().ok().expect("recursive `block_on`");

//         // Create the task context.
//         let cx = &mut Context::from_waker(&waker);

//         // Keep polling the future until completion.
//         loop {
//             match future.as_mut().poll(cx) {
//                 Poll::Ready(output) => return output,
//                 Poll::Pending => parker.park(),
//             }
//         }
//     })
// }


fn bench_block_on(c: &mut Criterion) {
    for i in &[0, 10, 50] {
        c.bench_with_input(BenchmarkId::new("my_block_on", i), i, |b, i| {
            b.iter(|| executor::block_on(Yields(*i)));
        });
        c.bench_with_input(BenchmarkId::new("futures_block_on", i), i, |b, i| {
            b.iter(|| futures::executor::block_on(Yields(*i)));
        });
    }
}


struct Yields(u32);

impl Future for Yields {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 == 0 {
            Poll::Ready(())
        } else {
            self.0 -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

criterion_group!(benches, bench_block_on);
criterion_main!(benches);
