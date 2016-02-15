// Copyright 2016 taskqueue developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

use std::boxed::FnBox;
use std::fmt;
use std::thread;
use std::sync::atomic::{ATOMIC_USIZE_INIT, AtomicUsize, Ordering};

use mioco::{self, ExitStatus, Config, Mioco};
use mioco::mail::*;

use future::{Future, FutureInternal, FutureGuard};
use group::Group;
use queue::{Queue, QueueId, LoopResult};

use util::mioco_handler::{Userdata, blocking_mioco_run_loop, new_coroutine};
use util::unsafe_wrap::NotThreadSafe;
use util::stack::Stack;

static ID: AtomicUsize = ATOMIC_USIZE_INIT;

enum Command
{
    Run(Box<FnBox() + Send + 'static>),
    Wait(MailboxInnerEnd<ExitStatus>),
    End,
}

/// Queue executing Tasks serially, non-overlapping in queued Order
///
/// ## Properties
/// - executes tasks in serial order
/// - no tasks may overlap
/// - they never change their native background thread (but may share it)
/// - the tasks are executed in order of queuing
/// - safety against deadlocks from recursive queueing (see example)
///
/// Through these guarantees SerialQueues may bound to a type that is **not** Send or Sync
/// and provide easy thread safe access to this critcal resource.
/// Such a SerialQueue is called [*BoundSerialQueue*](./struct.BoundSerialQueue.html).
///
/// ## Example
///
/// ```rust
/// # use taskqueue::*;
/// init_main(|main| {
///     let thread_one = SerialQueue::new();
///     let thread_two = SerialQueue::new();
///
///     let future_one = thread_one.async(|| {
///         42
///     });
///     let future_two = thread_two.async(|| {
///         96
///     });
///
///     println!("Although this is happening in main,");
///     main.async(|| {
///         println!("this task is running before...");
///     });
///     main.sync(|| {
///         println!("...this task and...");
///         assert_eq!(future_one.get() + future_two.get(), 138);
///     });
///     println!("...this is running last");
/// });
/// ```
pub struct SerialQueue
{
    id: usize,
    tx: MailboxOuterEnd<Command>,
    deadlock_tx: MailboxOuterEnd<()>,
}

impl PartialEq for SerialQueue
{
    fn eq(&self, other: &Self) -> bool
    {
        self.id == other.id
    }
}

impl fmt::Debug for SerialQueue
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "SerialQueue ({})", self.id)
    }
}

unsafe impl Send for SerialQueue
{}
unsafe impl Sync for SerialQueue
{}

impl SerialQueue
{
    /// Create a new SerialQueue and assign it to the global thread pool
    pub fn new() -> SerialQueue
    {
        let (tx, rx) = mailbox();
        let (deadlock_tx, deadlock_rx) = mailbox();

        let internal_tx = tx.clone();
        new_coroutine(move || {
            SerialQueue::do_loop(internal_tx, rx, deadlock_rx);
            Ok(())
        });

        let queue = SerialQueue {
            id: ID.fetch_add(1, Ordering::SeqCst),
            tx: tx,
            deadlock_tx: deadlock_tx,
        };
        info!("Queue created ({:?})", queue);
        queue
    }

    /// Create a new SerialQueue and assign it solely to a newly created OS Thread
    ///
    /// A SerialQueue created through this method will spawn a new native OS Thread
    /// and the queue will be the only utilizing it. The thread will be destructed,
    /// when the queue is dropped.
    ///
    /// The purpose of this constructor is to provide a way to use blocking IO with TaskQueue.
    /// The use of this method however is discouraged, as the new thread may influence
    /// the scheduler negatively and evented IO, where possible performs a lot better in combination
    /// with the TaskQueue library
    pub fn new_native() -> SerialQueue
    {
        let (tx, rx) = mailbox();
        let (deadlock_tx, deadlock_rx) = mailbox();

        let internal_tx = tx.clone();

        thread::spawn(move || {
            Mioco::new_configured({
                let mut config = Config::new();
                config.set_thread_num(1);
                config
            }).start(move || {
                SerialQueue::do_loop(internal_tx, rx, deadlock_rx);
                Ok(())
            });
        });

        let queue = SerialQueue {
            id: ID.fetch_add(1, Ordering::SeqCst),
            tx: tx,
            deadlock_tx: deadlock_tx,
        };
        info!("Native Queue created ({:?})", queue);
        queue
    }

    fn do_loop(queue_tx: MailboxOuterEnd<Command>,
               rx: MailboxInnerEnd<Command>,
               deadlock_rx: MailboxInnerEnd<()>)
    {

        debug!("loop: spawing serial loop");

        loop {
            trace!("loop: next iteration");
            match rx.read() {
                Command::End => break,
                Command::Wait(routine) => {
                    trace!("loop: handling previous deadlocked coroutine");
                    let tx_clone = queue_tx.clone();
                    loop {
                        select!(
                                routine:r => {
                                    if routine.try_read().is_some() {
                                        trace!("loop: task ended");
                                        break;
                                    } else {
                                        continue;
                                    }
                                },
                                deadlock_rx:r => {
                                    if deadlock_rx.try_read().is_some() {
                                        trace!("loop: deadlock detected");
                                        tx_clone.send(Command::Wait(routine));
                                        break;
                                    } else {
                                        continue;
                                    }
                                },
                            );
                    }
                }
                Command::Run(task) => {
                    let tx_clone = queue_tx.clone();
                    mioco::set_children_userdata(Some(Userdata::SameThread));
                    let routine = mioco::spawn_ext(move || {
                                      trace!("loop: spawned new coroutine for task");
                                      task.call_box(());
                                      Ok(())
                                  })
                                      .exit_notificator();
                    trace!("loop: wait for deadlock notification or coroutine finish");
                    loop {
                        select!(
                                routine:r => {
                                    if routine.try_read().is_some() {
                                        trace!("loop: task ended");
                                        break;
                                    } else {
                                        continue;
                                    }
                                },
                                deadlock_rx:r => {
                                    if deadlock_rx.try_read().is_some() {
                                        trace!("loop: deadlock detected");
                                        tx_clone.send(Command::Wait(routine));
                                        break;
                                    } else {
                                        continue;
                                    }
                                },
                            );
                    }
                }
            }
        }

        debug!("loop: queue ended");
    }

    /// Bind this queue to a variable
    ///
    /// This function allows to create a `BoundSerialQueue`.
    /// Its purpose is to bind variables to a queue, so they can be used by the tasks submitted.
    ///
    /// # Example
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// let x = 5;
    /// let bound = queue.with(move || x);
    /// bound.scoped_with(|x| println!("{}", x));
    /// // x gets dropped inside the queues thread before the queue gets dropped
    /// ```
    ///
    /// You can create multiple bindings at once.
    /// Through tuples you may bind multiple variables at once.
    ///
    /// It is even possible to move the creation of the bound variable into the queue by creating
    /// it inside the passed constructor, which is then executed on the queue.
    /// And because SerialQueues never change their underlying OS Thread,
    /// this allows to use variables that are not Send and Sync in a thread-safe but shared way.
    ///
    /// # Example
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// # fn my_ffi_function() -> *mut () { &mut () };
    /// let bound = queue.with(|| {
    ///     let raw_ptr = my_ffi_function();
    ///     raw_ptr
    /// });
    /// bound.scoped_with(|raw_ptr| println!("{}", raw_ptr.is_null()));
    /// // raw_ptr gets dropped inside the queues thread.
    /// // This way raw_ptr is never moved between threads.
    /// ```
    pub fn with<'queue, R: 'static, F>(&'queue self,
                                       constructor: F)
                                       -> BoundSerialQueue<'queue, R>
        where F: FnOnce() -> R + Send
    {
        let binding = self.sync(move || NotThreadSafe::new(constructor()));
        BoundSerialQueue {
            queue: self,
            binding: binding,
        }
    }
}

impl Queue for SerialQueue
{
    fn async<R, F>(&self, operation: F) -> Future<R>
        where R: Send + 'static,
              F: FnOnce() -> R + Send + 'static
    {
        let (tx, rx) = mailbox();

        let operation: Box<FnBox() + Send + 'static> = Stack::assemble(self, move || {
            tx.send(operation());
        });
        debug!("Queue ({:?}) queued task", self);
        self.tx.send(Command::Run(operation));

        Future::new_from_serial(self, Some(self.deadlock_tx.clone()), rx)
    }
}

/// A bound SerialQueue holding a queue-bound variable
///
/// Create a BoundSerialQueue using `SerialQueue::with`.
/// BoundSerialQueue's hold variables that may be used through
/// tasks executed on this queue though `scoped_with`, `sync_with`
/// `foreach_with` or `loop_while_with`.
///
/// `async_with` cannot be provided, as the bound variable is
/// dropped, when the BoundSerialQueue gets dropped.
///
/// Internally BoundSerialQueue refer to the same queue, they were created from.
/// Multiple BoundSerialQueue's may exist for one queue at once.
pub struct BoundSerialQueue<'queue, T: 'static>
{
    queue: &'queue SerialQueue,
    binding: NotThreadSafe<T>,
}

impl<'queue, T: 'static> fmt::Debug for BoundSerialQueue<'queue, T>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result
    {
        write!(f, "BoundSerialQueue ({:p})", self)
    }
}

impl<'queue, T: 'static> BoundSerialQueue<'queue, T>
{
    /// Like `Queue::scoped` but provides a mutable reference to the bound variable
    ///
    /// # Safety
    ///
    /// The same rules as for `Queue::scoped` to apply.
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// let bound = queue.with(|| { "Hello".to_string() });
    /// let name = "Stack".to_string();
    /// bound.scoped_with(|message| { println!("{} {}!", message, name) });
    /// ```
    pub fn scoped_with<R, F>(&'queue self, operation: F) -> FutureGuard<R>
        where R: Send + 'static,
              F: FnOnce(&'queue mut T) -> R + Send + 'queue
    {
        self.queue.scoped(move || operation(unsafe { self.binding.get_mut() }))
    }

    /// Like `Queue::sync` but provides a mutable reference to the bound variable
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// let bound = queue.with(|| { "a bound queue".to_string() });
    /// bound.sync_with(|name| { println!("Hello {}", name) });
    /// ```
    pub fn sync_with<R, F>(&'queue self, operation: F) -> R
        where R: Send + 'static,
              F: FnOnce(&'queue mut T) -> R + Send + 'queue
    {
        self.queue.sync(move || operation(unsafe { self.binding.get_mut() }))
    }

    /// Like `Queue::foreach` but provides a mutable reference to the bound variable
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// # let queue = SerialQueue::new();
    /// let bound = queue.with(|| 2);
    /// let doubled: Vec<i32> = bound.foreach_with((0..20), |factor, x| x*factor).collect();
    /// # assert_eq!((0..20).map(|x| x*2).collect::<Vec<i32>>(), doubled);
    /// ```
    pub fn foreach_with<B, R, I, F>(&'queue self, mut iter: I, operation: F) -> Group<R>
        where B: Send,
              R: Send + 'queue,
              I: Iterator<Item = B> + Send,
              F: Fn(&'queue T, B) -> R + Send + Sync + 'queue
    {
        let mut group = Group::new();
        loop {
            match iter.next() {
                Some(x) => {
                    let op = &operation;
                    let binding = &self.binding;
                    group.scoped(self, move || op(unsafe { binding.get_mut() }, x))
                },
                None => break,
            }
        }
        group
    }

    /// Like `Queue::loop_while` but provides a mutable reference to the bound variable
    ///
    /// # Example
    ///
    /// ```
    /// # use taskqueue::*;
    /// use std::mem;
    ///
    /// let queue = SerialQueue::new();
    /// let bound = queue.with(|| (15, 25));
    ///
    /// let greatest_common_divisor = bound.loop_while_with(|tuple| {
    ///     let x = tuple.0;
    ///     let y = tuple.1;
    ///
    ///     if y == 0 {
    ///         LoopResult::Done(x.clone())
    ///     } else {
    ///         let remainder = x % y;
    ///         let mut new_tuple = (y, remainder);
    ///         mem::swap(tuple, &mut new_tuple);
    ///         LoopResult::Continue
    ///     }
    /// }).get();
    /// #
    /// # assert_eq!(5, greatest_common_divisor);
    /// ```
    pub fn loop_while_with<R, F>(&'queue self, operation: F) -> FutureGuard<R>
        where F: Fn(&'queue mut T) -> LoopResult<R> + Send + Sync + 'queue,
              R: Send + 'static,
    {
        self.queue.loop_while(move || operation(unsafe { self.binding.get_mut() }))
    }
}

impl<'a, T: 'static> Queue for BoundSerialQueue<'a, T>
{
    fn async<R, F>(&self, operation: F) -> Future<R>
        where R: Send + 'static,
              F: FnOnce() -> R + Send + 'static
    {
        self.queue.async(operation)
    }
}

impl<'queue, T: 'static> Drop for BoundSerialQueue<'queue, T>
{
    fn drop(&mut self)
    {
        let binding = self.binding.clone();
        self.queue.async(move || {
            unsafe {
                binding.drop();
            }
        });
    }
}

impl QueueId for SerialQueue
{
    fn id(&self) -> usize
    {
        self.id
    }
}

/// Convert the main thread into a SerialQueue
///
/// This function warps the current thread into a SerialQueue, that
/// is passed to the executed function, blocking the current thread
/// until the created Queue is done.
///
/// This function is only intended to be used on the main thread and
/// library creators should never need to use it.
///
/// If you need a queue based on a newly created OS thread use `SerialQueue::new_native()`.
///
/// # Example
///
/// ```
/// # use taskqueue::*;
/// fn main() {
///     init_main(|main_queue| {
///         // start using it!
///     })
/// }
/// ```
pub fn init_main<F: FnOnce(SerialQueue) + Send + 'static>(start: F)
{
    blocking_mioco_run_loop(move || {
        mioco::set_children_userdata(Some(Userdata::RoundRobin));

        let (tx, rx) = mailbox::<Command>();
        let (deadlock_tx, deadlock_rx) = mailbox::<()>();

        let new_queue = SerialQueue {
            id: ID.fetch_add(1, Ordering::SeqCst),
            tx: tx.clone(),
            deadlock_tx: deadlock_tx.clone(),
        };

        tx.send(Command::Run(
            Stack::assemble_main(new_queue, start)
        ));

        info!("Main Queue ready!");

        SerialQueue::do_loop(tx, rx, deadlock_rx);
        Ok(())
    })
}

impl Drop for SerialQueue
{
    fn drop(&mut self)
    {
        trace!("Dropping {:?}", self);
        self.tx.send(Command::End);
    }
}
