// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::collections::VecDeque;
use std::fmt;
use std::iter::IntoIterator;
use std::vec::IntoIter;

use future::{Future, FutureGuard};
use Queue;

enum FutureLike<R: Send + 'static>
{
    Future(Future<R>),
    FutureGuard(FutureGuard<R>),
}

impl<R: Send + 'static> fmt::Debug for FutureLike<R>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        match self {
            &FutureLike::Future(ref x) => write!(f, "{:?}", x),
            &FutureLike::FutureGuard(ref x) => write!(f, "{:?}", x),
        }
    }
}

impl<R: Send + 'static> FutureLike<R>
{
    fn get(self) -> R
    {
        match self {
            FutureLike::Future(x) => x.get(),
            FutureLike::FutureGuard(x) => x.get(),
        }
    }
}

/// Group provides a way to synchronize multiple Tasks
///
/// Using Group you can block on multiple tasks at once
/// or handle them in order across multiple queues.
pub struct Group<R: Send + 'static>
{
    handlers: VecDeque<FutureLike<R>>,
}

impl<R: Send + 'static> fmt::Debug for Group<R>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        try!(write!(f, "Group ["));
        for x in self.handlers.iter() {
            try!(write!(f, "{:?}", x));
        }
        write!(f, "]")
    }
}

impl<R: Send + 'static> Group<R>
{
    /// Create a new Group
    pub fn new() -> Group<R>
    {
        Group { handlers: VecDeque::new() }
    }

    /// Add a task executed on the passed `queue` to the `Group`.
    /// Behaves otherwise like `Queue::async`.
    ///
    /// # Arguments
    ///
    /// *queue* - The Queue to execute upon
    /// *task* - The Task to be executed
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// let queue_one = SerialQueue::new();
    /// let queue_two = SerialQueue::new();
    /// let mut group = Group::new();
    /// group.async(&queue_one, || {
    ///     // some heavy computation
    /// });
    /// group.async(&queue_two, || {
    ///     // another heavy computation
    /// });
    /// group.wait();
    /// ```
    pub fn async<T, F>(&mut self, queue: &T, task: F)
        where F: FnOnce() -> R + Send + 'static,
              T: Queue
    {
        let future: Future<R> = queue.async(move || task());
        self.handlers.push_back(FutureLike::Future(future));
    }

    /// Add a task executed on the passed `queue` to the `Group`.
    /// Behaves otherwise like `Queue::scoped`, so it may capture
    /// stack variables.
    ///
    /// The Guard is not lost when dropping the Group, but is instead
    /// also dropped blocking till the end of the task execution.
    ///
    /// # Safety
    ///
    /// Same rules as for `Queue::scoped` `FutureGuard` return value
    /// do apply to the Group, when this function was used.
    pub fn scoped<'queue, T, F>(&mut self, queue: &'queue T, task: F)
        where F: FnOnce() -> R + Send + 'queue,
              T: Queue
    {
        let future: FutureGuard<R> = queue.scoped(move || task());
        self.handlers.push_back(FutureLike::FutureGuard(future));
    }

    /// Blocks the current Queue until all tasks finished execution and returns it as Iterator
    pub fn wait(self) -> IntoIter<R>
    {
        let mut results = vec![];
        for x in self.handlers.into_iter() {
            results.push(x.get());
        }
        results.into_iter()
    }
}

impl<R: Send + 'static> IntoIterator for Group<R>
{
    type Item = R;
    type IntoIter = IntoIter<R>;
    fn into_iter(self) -> Self::IntoIter
    {
        self.wait()
    }
}
