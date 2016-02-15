// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::fmt;
use std::mem;

use mioco::mail::{MailboxInnerEnd, MailboxOuterEnd};

use SerialQueue;

use util::stack::Stack;
use util::mioco_handler::read_from_mailbox;

/// Future holding the variable of a maybe to be executed task
pub struct Future<R: Send + 'static>
{
    rx: MailboxInnerEnd<R>,
    deadlock_tx: Option<MailboxOuterEnd<()>>,
    frozen_active_stack: Option<Stack>,
    frozen_assembled_stack: Option<Stack>,
}
unsafe impl<R: Send + 'static> Send for Future<R>
{}

/// Future holding the variable of a maybe to be executed task.
/// Guards the Task and waits for it on Drop.
pub struct FutureGuard<R: Send + 'static>(Option<Future<R>>);

impl<R: Send + 'static> fmt::Debug for Future<R>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f,
               "Future (Queue: ({:?}))",
               self.frozen_active_stack)
    }
}

impl<R: Send + 'static> fmt::Debug for FutureGuard<R>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f,
               "FutureGuard (Queue: ({:?}))",
               self.0.as_ref().unwrap().frozen_active_stack)
    }
}

impl<R: Send + 'static> Future<R>
{
    /// Blocks the current Queue until the value can be recieved.
    /// This may happen instantly or take a long time, completely depending
    /// on the task and the amount of work the corresponding queue has to face.
    pub fn get(self) -> R
    {
        self.deadlock_detection();

        read_from_mailbox(self.rx)
    }
}

impl<R: Send + 'static> FutureGuard<R>
{

    /// Blocks the current Queue until the value can be recieved.
    /// This may happen instantly or take a long time, completely depending
    /// on the task and the amount of work the corresponding queue has to face.
    pub fn get(mut self) -> R
    {
        let mut swap = None;
        mem::swap(&mut self.0, &mut swap);
        swap.unwrap().get()
    }
}

impl<R: Send + 'static> Drop for FutureGuard<R>
{
    fn drop(&mut self)
    {
        if self.0.is_some() {
            let mut swap = None;
            mem::swap(&mut self.0, &mut swap);
            swap.unwrap().get();
        }
    }
}

pub trait FutureInternal<R: Send + 'static>
{
    fn new_from_serial(queue: &SerialQueue,
                     deadlock_tx: Option<MailboxOuterEnd<()>>,
                     rx: MailboxInnerEnd<R>)
                     -> Future<R>;

    fn new_from_concurrent(deadlock_tx: Option<MailboxOuterEnd<()>>,
                     rx: MailboxInnerEnd<R>)
                     -> Future<R>;

    fn guard(self) -> FutureGuard<R>;
    unsafe fn change<N: Send + 'static>(self, rx: MailboxInnerEnd<N>) -> Future<N>;
    fn deadlock_detection(&self);
}

pub trait FutureGuardInternal<R: Send + 'static>
{
    unsafe fn unwrap(mut self) -> Future<R>;
}

impl<R: Send + 'static> FutureInternal<R> for Future<R>
{
    fn new_from_serial(queue: &SerialQueue,
                     deadlock_tx: Option<MailboxOuterEnd<()>>,
                     rx: MailboxInnerEnd<R>)
                     -> Future<R>
    {
        Future {
            rx: rx,
            deadlock_tx: deadlock_tx,
            frozen_active_stack: Stack::active_stack(),
            frozen_assembled_stack: Some(Stack::assembled_stack(queue)),
        }
    }

    fn new_from_concurrent(deadlock_tx: Option<MailboxOuterEnd<()>>,
                     rx: MailboxInnerEnd<R>)
                     -> Future<R>
    {
        Future {
            rx: rx,
            deadlock_tx: deadlock_tx,
            frozen_active_stack: Stack::active_stack(),
            frozen_assembled_stack: None,
        }
    }

    fn guard(self) -> FutureGuard<R>
    {
        FutureGuard(Some(self))
    }

    unsafe fn change<N: Send + 'static>(self, rx: MailboxInnerEnd<N>) -> Future<N>
    {
        Future {
            rx: rx,
            deadlock_tx: self.deadlock_tx,
            frozen_active_stack: self.frozen_active_stack,
            frozen_assembled_stack: self.frozen_assembled_stack,
        }
    }

    fn deadlock_detection(&self) {
        trace!("future: get {:?}, {:?}", self.frozen_active_stack, self.frozen_assembled_stack);
        if self.deadlock_tx.is_some() && self.frozen_active_stack.is_some() && self.frozen_assembled_stack.is_some() {
            let tx = self.deadlock_tx.as_ref().unwrap();
            for _ in 0..self.frozen_active_stack.as_ref().unwrap().loop_detection(self.frozen_assembled_stack.as_ref().unwrap()) {
                trace!("future: deadlock notification");
                tx.send(());
            }
        }
    }
}

impl<R: Send + 'static> FutureGuardInternal<R> for FutureGuard<R>
{
    unsafe fn unwrap(mut self) -> Future<R> {
        let mut swap = None;
        mem::swap(&mut self.0, &mut swap);
        swap.unwrap()
    }
}
