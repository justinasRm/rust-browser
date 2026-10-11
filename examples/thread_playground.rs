use std::{
    sync::mpsc::{self, TryRecvError},
    thread,
    time::Duration,
};

fn main() {
    let (sender, receiver) = mpsc::channel();

    let worker = thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));

        sender.send("I'm from worker").unwrap();
    });

    loop {
        match receiver.try_recv() {
            Ok(message) => {
                println!("Received: {message}");
                break;
            }
            Err(TryRecvError::Empty) => {
                println!("Not ready; main can do other work");
            }
            Err(TryRecvError::Disconnected) => {
                println!("Worker disconnected without a message");
                break;
            }
        }

        thread::sleep(Duration::from_millis(500));
    }

    worker.join().unwrap();
}
