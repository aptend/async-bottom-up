use async_bottom_up::task::{executor, Task};
use async_std::io;

async fn async_numebr() -> u32 {
    42
}

async fn find_answer() {
    let n = async_numebr().await;
    println!("async_number: {}", n);
}

async fn read_user_input() {
    let stdin = io::stdin();
    let mut line = String::new();
    loop {
        stdin.read_line(&mut line).await.unwrap();
        println!("{}", line);
        line.clear();
    }
}

fn main() {
    let mut exec = executor::Executor::new();
    exec.spawn(Task::new(find_answer()));
    exec.spawn(Task::new(read_user_input()));
    exec.run();
}
