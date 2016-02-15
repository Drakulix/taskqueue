// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::boxed::FnBox;
use std::fmt;
use std::mem;

use mioco::mail::*;

use group::Group;
use future::{Future, FutureGuard, FutureInternal, FutureGuardInternal};

mod serial;
mod concurrent;

pub use self::serial::{SerialQueue, BoundSerialQueue, init_main};
pub use self::concurrent::ConcurrentQueue;

/// Helper struct for `Queue::loop_while`
///
/// Inside your loop body return either `Continue`
/// to let your loop continue running or return `Done`
/// with the return value, that shall be observable
/// through the FutureGuard `loop_while` returns.
#[derive(Debug)]
pub enum LoopResult<R: Send + 'static>
{
    /// Let loop continue
    Continue,
    /// Abort loop and return value
    Done(R),
}

/// Basic Queue Trait defining all possible operations on Queues
///
/// Queues implement this, so you may use a unified Interface for all Queues.
pub trait Queue: Send + Sync + Sized + fmt::Debug
{
    /// Runs a task asynchrously on this queue
    ///
    /// `async` may not capture values on the current stack, as the closure may live longer then
    /// the current stack. For example, if you drop the Future and the Queue directly afterwards.
    /// Your task will still be executed in that case.
    ///
    /// # Arguments
    ///
    /// *task* - Task to be run by the queue
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// queue.async(|| { println!("Hello Queue!") });
    /// ```
    ///
    /// Note that this limitation does not mean you cannot *move* variables, this is totally fine:
    ///
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// let name = "Welt".to_string();
    /// queue.async(move || { println!("Hello {}!", name) });
    /// ```
    fn async<R, F>(&self, task: F) -> Future<R>
        where R: Send + 'static,
              F: FnOnce() -> R + Send + 'static;

    /// Runs a task asynchrously on this queue allowing capture of stack variables
    ///
    /// `scoped` may capture stack variables. To ensure the task may not outlive the current
    /// stack scope the returned FutureGuard blocks on drop, waiting for the task to finish
    /// executing. To avoid this behavior use `async`.
    ///
    /// # Safety
    /// Note that it is unsafe to leak the returned `FutureGuard`. Neither mem::forget nor cyclic
    /// RA II structures should be used in combination with FutureGuard as a leak may result
    /// in a dangeling pointer (read crash). This is the same reason thread::scoped was removed
    /// from the standard library. Read more here [*24292*](https://github.com/rust-lang/rust/issues/24292).
    ///
    /// # Arguments
    ///
    /// *task* - Task to be run on this queue
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// let name = "Stack".to_string();
    /// queue.scoped(|| { println!("Hello {}!", name) }); //note the missing `move`
    /// // join to the task occurs here
    /// ```
    fn scoped<'queue, R, F>(&'queue self, task: F) -> FutureGuard<R>
        where R: Send + 'static,
              F: FnOnce() -> R + Send + 'queue
    {
        let task: Box<FnBox() -> R + Send + 'static> = {
            unsafe {
                let boxed: Box<FnBox() -> R + Send + 'queue> = Box::new(task);
                let ptr: *mut FnBox() -> R = mem::transmute(Box::into_raw(boxed));
                Box::from_raw(mem::transmute(ptr))
            }
        };
        self.async(task).guard()
    }

    /// Runs a task synchronously on this queue
    ///
    /// `sync` returns after the task was executed blocking the current queue.
    /// As a result capturing of stack variables is totally safe with this function
    /// and no Future is required.
    ///
    /// # Arguments
    ///
    /// *task* - Task to be run on this queue
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// println!("Hello {}", queue.sync(|| { "from another queue".to_string() }));
    /// ```
    fn sync<'queue, R, F>(&'queue self, task: F) -> R
        where R: Send + 'static,
              F: FnOnce() -> R + Send + 'queue
    {
        // This crates a little bit more stack objects (2 instead of 1)
        // then a direct implementation. But I rather rely on the compiler
        // to optimize that away, then maintaining to loop_detections

        let f = self.scoped(task);
        f.get()
    }

    /// Runs a task once for each iterator elem on this queue
    ///
    /// `foreach` queues one task multiple times for each iterator element
    /// of the iterator passed consuming it. It returns a Group containing the
    /// FutureGuards of all executions. Note that Group also implements the Itertor trait.
    ///
    /// # Arguments
    ///
    /// *iter* - Iterator to be consumed
    /// *task* - Task to be run for `iter` on this queue
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// # let queue = ConcurrentQueue::new();
    /// let squared: Vec<i32> = queue.foreach((0..20), |x| x*x).wait().collect();
    /// # assert_eq!((0..20).map(|x| x*x).collect::<Vec<i32>>(), squared);
    /// ```
    fn foreach<'queue, B, R, I, F>(&'queue self, mut iter: I, task: F) -> Group<R>
        where B: Send,
              R: Send + 'queue,
              I: Iterator<Item = B> + Send,
              F: Fn(B) -> R + Send + Sync + 'queue
    {
        let mut group = Group::new();
        loop {
            match iter.next() {
                Some(x) => {
                    let op = &task;
                    group.scoped(self, move || op(x))
                },
                None => break,
            }
        }
        group
    }

    /// Loops a task on this queue while a condition is hold
    ///
    /// `loop_while` queues a task again and again until it returns `Done`.
    /// Note that this is perferred over:
    ///
    /// ```no_run
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// queue.async(|| {
    ///     loop { //this is bad
    ///          /* your task */
    ///     }
    /// });
    /// ```
    ///
    /// because `loop_while` allows other queued tasks to run in the meantime, because the task
    /// is queued again and again, while the code above runs as one long task.
    ///
    /// Note that this is especially useful in combination with `SerialQueue::init_main` to create
    /// a main thread application run loop.
    ///
    /// # Arguments
    ///
    /// *task* - Task to be looped on this queue
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// # struct Program;
    /// # impl Program {
    /// #     fn did_exit(&self) -> Option<i32> { Some(0) }
    /// #     fn tick(&self) {}
    /// # }
    /// let program = Program;
    ///
    /// queue.loop_while(move || {
    ///    match program.did_exit() {
    ///        Some(exit_code) => LoopResult::Done(exit_code),
    ///        None => {
    ///            program.tick();
    ///            LoopResult::Continue
    ///       },
    ///    }
    /// }).get();
    /// ```
    fn loop_while<'queue, R, F>(&'queue self, task: F) -> FutureGuard<R>
        where F: Fn() -> LoopResult<R> + Send + Sync + 'queue,
              R: Send + 'static,
    {
        fn op<'queue, R, F, T>(queue: &'queue T, tx: MailboxOuterEnd<R>, operation: F)
            where F: Fn() -> LoopResult<R> + Send + Sync + 'queue,
                  R: Send + 'static,
                  T: Queue + 'queue,
        {
            match operation() {
                LoopResult::Continue => unsafe { queue.scoped(move || { op(queue, tx, operation); }).unwrap().deadlock_detection() },
                LoopResult::Done(x) => tx.send(x),
            };
        }

        let (tx, rx) = mailbox::<R>();
        unsafe { self.scoped(move || { op(self, tx, task); }).unwrap().change(rx).guard() }
    }
}

pub trait QueueId
{
    fn id(&self) -> usize;
}
