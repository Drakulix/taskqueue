// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::boxed::FnBox;
use std::collections::VecDeque;
use std::io;
use std::thread;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::*;

use mioco::{self, Mioco, Config, Scheduler, SchedulerThread, Handler, CoroutineControl};
use mioco::mail::*;

lazy_static! {
    static ref INSTANCE: MailboxOuterEnd<Box<FnBox() + Send + 'static>> = {
        let (tx, rx) = sync_channel(0);
        thread::spawn(move || {
            Mioco::new_configured({
                let mut config = Config::new();
                config.set_scheduler(Box::new(FifoScheduler::new()));
                config.set_userdata(Userdata::RoundRobin);
                config.set_catch_panics(false);
                config
            }).start(move || {
                let(tx_fn, rx_fn) = mailbox::<Box<FnBox() + Send + 'static>>();
                tx.send(tx_fn).unwrap();
                loop {
                    rx_fn.read()();
                }
            });
        });
        rx.recv().unwrap()
    };
}

pub enum Userdata
{
    RoundRobin,
    SameThread,
}

pub fn new_coroutine<F: FnOnce() -> io::Result<()> + Send + 'static>(f: F)
{
    INSTANCE.send(Box::new(move || {
        mioco::set_children_userdata(Some(Userdata::RoundRobin));
        mioco::spawn(f);
    }));
}

pub fn read_from_mailbox<T: Send + 'static>(rx: MailboxInnerEnd<T>) -> T {
    match mioco::in_coroutine() {
        true => rx.read(),
        false => {
            let (tx, rx_result) = sync_channel(0);
            new_coroutine(move || {
                tx.send(rx.read()).unwrap();
                Ok(())
            });
            rx_result.recv().unwrap()
        }
    }
}

pub fn blocking_mioco_run_loop<F: FnOnce() -> io::Result<()> + Send + 'static>(f: F) {
    Mioco::new_configured({
        let mut config = Config::new();
        config.set_thread_num(1);
        config.set_catch_panics(false);
        config
    }).start(f);
}

struct FifoScheduler {
    thread_num: Arc<AtomicUsize>,
}

impl FifoScheduler {
    pub fn new() -> Self {
        FifoScheduler { thread_num: Arc::new(AtomicUsize::new(0)) }
    }
}

struct FifoSchedulerThread {
    thread_i: usize,
    thread_num: Arc<AtomicUsize>,
    delayed: VecDeque<CoroutineControl>,
}

impl Scheduler for FifoScheduler {
    fn spawn_thread(&self) -> Box<SchedulerThread> {
        self.thread_num.fetch_add(1, Ordering::Relaxed);
        Box::new(FifoSchedulerThread {
            thread_i: 0,
            thread_num: self.thread_num.clone(),
            delayed: VecDeque::new(),
        })
    }
}

impl FifoSchedulerThread {
    fn thread_next_i(&mut self) -> usize {
        self.thread_i += 1;
        if self.thread_i >= self.thread_num() {
            self.thread_i = 0;
        }
        self.thread_i
    }

    fn thread_num(&self) -> usize {
        self.thread_num.load(Ordering::Relaxed)
    }
}

impl SchedulerThread for FifoSchedulerThread {
    fn spawned(&mut self,
               event_loop: &mut mioco::mio::EventLoop<Handler>,
               coroutine_ctrl: CoroutineControl) {
        match coroutine_ctrl.get_userdata::<Userdata>() {
            Some(&Userdata::SameThread) => {
                trace!("Using newly spawn Coroutine on current thread");
                coroutine_ctrl.resume(event_loop)
            },
            _ => {
                let thread_i = self.thread_next_i();
                trace!("Migrating newly spawn Coroutine to thread {}", thread_i);
                coroutine_ctrl.migrate(event_loop, thread_i);
            }
        }
    }

    fn ready(&mut self,
             event_loop: &mut mioco::mio::EventLoop<Handler>,
             coroutine_ctrl: CoroutineControl) {
        if coroutine_ctrl.is_yielding() {
            self.delayed.push_back(coroutine_ctrl);
        } else {
            coroutine_ctrl.resume(event_loop);
        }
    }

    fn tick(&mut self, event_loop: &mut mioco::mio::EventLoop<Handler>) {
        let len = self.delayed.len();
        for _ in 0..len {
            let coroutine_ctrl = self.delayed.pop_front().unwrap();
            coroutine_ctrl.resume(event_loop);
        }
    }
}
