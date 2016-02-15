// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::boxed::FnBox;
use std::fmt;

use SerialQueue;
use ::queue::QueueId;

scoped_thread_local!(static QUEUE_STACK: Stack);

#[derive(Clone)]
pub struct Stack
{
    queue: Option<usize>,
    parent: Option<Box<Stack>>,
}

impl fmt::Debug for Stack
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        let mut current = self;
        while {
            try!(write!(f, "({:?})", current.queue));
            let res = current.parent.is_some();
            if res {
                current = current.parent.as_ref().unwrap();
            }
            res
        } {}
        Ok(())
    }
}

impl PartialEq for Stack
{
    fn eq(&self, other: &Self) -> bool
    {
        self.queue == other.queue
    }
}

impl Stack
{
    pub fn active_stack() -> Option<Stack>
    {
        match QUEUE_STACK.is_set() {
            true => QUEUE_STACK.with(move |x| Some(x.clone())),
            false => None,
        }
    }

    pub fn assembled_stack(queue: &SerialQueue) -> Stack
    {
        Stack {
            queue: Some(queue.id()),
            parent: match QUEUE_STACK.is_set() {
                true => {
                    QUEUE_STACK.with(move |x| {
                        Some(Box::new(x.clone()))
                    })
                },
                false => None,
            }
        }
    }

    pub fn disassembled_stack() -> Stack
    {
        Stack {
            queue: None,
            parent: match QUEUE_STACK.is_set() {
                true => {
                    QUEUE_STACK.with(move |x| {
                        Some(Box::new(x.clone()))
                    })
                },
                false => None,
            },
        }
    }

    pub fn assemble<'a, F, R>(queue: &SerialQueue, operation: F) -> Box<FnBox() -> R + Send + 'a>
        where F: FnOnce() -> R + Send + 'a
    {
        let stack = Stack::assembled_stack(queue);
        Box::new(move || QUEUE_STACK.set(&stack, operation))
    }

    pub fn disassemble<'a, F, R>(operation: F) -> Box<FnBox() -> R + Send + 'a>
        where F: FnOnce() -> R + Send + 'a
    {
        let stack = Stack::disassembled_stack();
        Box::new(move || QUEUE_STACK.set(&stack, operation))
    }

    pub fn assemble_main<'a, F, R>(queue: SerialQueue, operation: F) -> Box<FnBox() -> R + Send + 'a>
        where F: FnOnce(SerialQueue) -> R + Send + 'a
    {
        let stack = Stack::assembled_stack(&queue);
        Box::new(move || QUEUE_STACK.set(&stack, move || operation(queue)))
    }

    pub fn loop_detection(&self, other: &Stack) -> usize
    {
        let mut count = 0;
        let mut current = self;
        while {

            if current == other {
                count += 1;
            }

            let res = current.parent.is_some();
            if res {
                current = current.parent.as_ref().unwrap();
            }
            res
        } {}
        count
    }
}
