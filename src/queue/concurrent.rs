// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::boxed::FnBox;
use std::fmt;

use mioco;
use mioco::mail::*;

use future::{Future, FutureInternal};
use Queue;

use util::mioco_handler::{Userdata, new_coroutine};
use util::stack::Stack;

enum Command
{
    Run(Box<FnBox() + Send + 'static>),
    End,
}

/// Queue executing tasks in parallel
///
/// ## Properties
/// - executes a new task as soon as possible on the next thread
/// - tasks may overlap
/// - tasks may execute on totally different threads
/// - safety against deadlocks from recursive queueing (see SerialQueue example)
///
/// This disallows type binding but enables very easy parallization where appropriate.
///
/// ## Example
///
/// ```rust
/// # use taskqueue::*;
/// # use std::iter::Iterator;
/// let queue = ConcurrentQueue::new();
///
/// let mut result = Vec::with_capacity(4096);
/// for x in 0..4096 {
///     result.push(queue.async(move || x*x));
/// }
/// let result: Vec<i32> = result.into_iter().map(|x| x.get()).collect();
/// ```
pub struct ConcurrentQueue
{
    tx: MailboxOuterEnd<Command>,
}

impl fmt::Debug for ConcurrentQueue
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "ConcurrentQueue")
    }
}
unsafe impl Send for ConcurrentQueue
{}
unsafe impl Sync for ConcurrentQueue
{}

impl ConcurrentQueue
{
    /// Create a new ConcurrentQueue and executing tasks on the global thread pool
    pub fn new() -> ConcurrentQueue
    {
        let (tx, rx) = mailbox();
        new_coroutine(move || {
            loop {
                match rx.read() {
                    Command::Run(task) => {
                        mioco::set_children_userdata(Some(Userdata::RoundRobin));
                        mioco::spawn(move || {
                            task.call_box(());
                            Ok(())
                        });
                    }
                    Command::End => {
                        break;
                    }
                };
            }
            Ok(())
        });
        let queue = ConcurrentQueue { tx: tx };
        info!("Queue created ({:?})", queue);
        queue
    }
}

impl Queue for ConcurrentQueue
{
    fn async<R, F>(&self, operation: F) -> Future<R>
        where R: Send + 'static,
              F: FnOnce() -> R + Send + 'static
    {
        let (tx, rx) = mailbox();

        let operation: Box<FnBox() + Send + 'static> = Stack::disassemble(move || {
            tx.send(operation())
        });
        debug!("Queue ({:?}) queued task", self);
        self.tx.send(Command::Run(operation));

        Future::new_from_concurrent(None, rx)
    }
}

impl Drop for ConcurrentQueue
{
    fn drop(&mut self)
    {
        trace!("Dropping {:?}", self);
        self.tx.send(Command::End);
    }
}
