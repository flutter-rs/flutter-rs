//! The platform-thread task runner the engine posts its own tasks to.

use crate::FlutterEngineWeakRef;
use flutter_engine_sys::{FlutterEngineGetCurrentTime, FlutterTask};
use log::debug;
use parking_lot::{Mutex, MutexGuard};
use priority_queue::PriorityQueue;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::thread;
use std::thread::ThreadId;
use std::time::{Duration, Instant};

/// Woken by the engine whenever it has posted a task, so the embedder's event
/// loop knows to call
/// [`FlutterEngine::execute_platform_tasks`](crate::FlutterEngine::execute_platform_tasks).
///
/// `wake` is called from engine threads, so it must be thread-safe and must not
/// block.
pub trait TaskRunnerHandler {
    /// Nudge the embedder's event loop.
    fn wake(&self);
}

pub(crate) struct TaskRunnerInner {
    engine: FlutterEngineWeakRef,
    pub(crate) handler: Arc<dyn TaskRunnerHandler + Send + Sync>,
    thread_id: ThreadId,
    tasks: PriorityQueue<Task, TaskPriority>,
}

/// The queue of engine tasks waiting to run on the platform thread.
///
/// Created by the engine, not by embedders; implement [`TaskRunnerHandler`] to
/// be told when it has work.
#[derive(Clone)]
pub struct TaskRunner {
    pub(crate) inner: Arc<Mutex<TaskRunnerInner>>,
}

impl core::fmt::Debug for TaskRunner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let inner = self.inner.lock();
        f.debug_struct("TaskRunner")
            .field("thread_id", &inner.thread_id)
            .field("queued_tasks", &inner.tasks.len())
            .finish_non_exhaustive()
    }
}

impl TaskRunnerInner {
    pub(crate) fn runs_task_on_current_thread(&self) -> bool {
        thread::current().id() == self.thread_id
    }
}

impl TaskRunner {
    pub(crate) fn new(handler: Arc<dyn TaskRunnerHandler + Send + Sync>) -> Self {
        let thread_id = thread::current().id();
        debug!("task runner created on thread {:?}", thread_id);
        Self {
            inner: Arc::new(Mutex::new(TaskRunnerInner {
                engine: FlutterEngineWeakRef::default(),
                handler,
                thread_id,
                tasks: PriorityQueue::new(),
            })),
        }
    }

    pub(crate) fn init(&self, engine: FlutterEngineWeakRef) {
        let mut inner = self.inner.lock();
        inner.engine = engine;
    }

    /// Run every task whose target time has passed, and report when the next
    /// one is due.
    pub(crate) fn execute_tasks(&self) -> Option<Instant> {
        let now = Instant::now();
        let mut expired_tasks = Vec::new();

        let engine = {
            let mut inner = self.inner.lock();
            let tasks = &mut inner.tasks;
            while let Some((_, priority)) = tasks.peek() {
                if priority.time > now {
                    break;
                }
                let Some((task, _)) = tasks.pop() else { break };
                expired_tasks.push(task);
            }
            // make sure to unlock mutex before actually running the tasks as they may post another task
            match inner.engine.upgrade() {
                Some(engine) => engine,
                None => {
                    debug!(
                        "the engine is gone, dropping {} expired task(s)",
                        expired_tasks.len()
                    );
                    return None;
                }
            }
        };

        // run tasks
        for task in expired_tasks {
            engine.run_task(&task.task);
        }

        // next task time
        let mut inner = self.inner.lock();
        let tasks = &mut inner.tasks;
        tasks.peek().map(|(_, priority)| priority.time)
    }

    fn flutter_time_to_instant(target_time_nanos: u64) -> Instant {
        // SAFETY: `FlutterEngineGetCurrentTime` takes no arguments, touches no
        // engine state and is callable from any thread.
        let current_time = unsafe { FlutterEngineGetCurrentTime() };
        let now = Instant::now();
        if current_time >= target_time_nanos {
            return now;
        }
        let nanos_timeout = target_time_nanos - current_time;
        now.checked_add(Duration::from_nanos(nanos_timeout))
            .unwrap_or_else(|| {
                log::warn!(
                    "task target time {target_time_nanos} overflows Instant; running it now"
                );
                now
            })
    }

    pub(crate) fn post_task(
        guard: &mut MutexGuard<TaskRunnerInner>,
        task: FlutterTask,
        target_time_nanos: u64,
    ) {
        static GLOBAL_ORDER: AtomicU64 = AtomicU64::new(0);
        let task_priority = TaskPriority {
            time: Self::flutter_time_to_instant(target_time_nanos),
            order: GLOBAL_ORDER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        };
        let task = Task { task };
        let tasks = &mut guard.tasks;
        tasks.push(task, task_priority);

        let handler = guard.handler.clone();

        // make sure to unlock the mutex before posting an event because the event handler may read the queue
        MutexGuard::unlocked(guard, move || {
            handler.wake();
        });
    }

    pub(crate) fn runs_task_on_current_thread(&self) -> bool {
        self.inner.lock().runs_task_on_current_thread()
    }

    pub(crate) fn wake(&self) {
        let handler = { self.inner.lock().handler.clone() };
        handler.wake();
    }
}

#[derive(Eq, PartialEq)]
struct TaskPriority {
    order: u64,
    time: Instant,
}

struct Task {
    task: FlutterTask,
}

// SAFETY: `FlutterTask.runner` is an opaque engine-owned handle that the
// embedder only ever compares (`ptr::eq`/`ptr::hash` below) and hands back to
// `FlutterEngineRunTask`; it is never dereferenced here. The engine documents
// its tasks as runnable from the thread that owns the queue, which is the
// platform thread, so moving the handle between threads is sound.
unsafe impl Send for Task {}

// SAFETY: see the `Send` impl above — the handle is only ever compared or
// passed back by value, so shared access cannot race.
unsafe impl Sync for Task {}

impl Ord for TaskPriority {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.time.cmp(&other.time) {
            Ordering::Equal => self.order.cmp(&other.order),
            ord => ord,
        }
    }
}

impl PartialOrd for TaskPriority {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Task {
    fn eq(&self, other: &Self) -> bool {
        self.task.task == other.task.task && std::ptr::eq(self.task.runner, other.task.runner)
    }
}

impl Eq for Task {}

impl Hash for Task {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.task.task.hash(state);
        std::ptr::hash(self.task.runner, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Earlier target times run first; ties break on post order, so tasks posted
    /// for the same instant keep their FIFO order.
    #[test]
    fn task_priority_orders_by_time_then_post_order() {
        let now = Instant::now();
        let later = now + Duration::from_millis(5);

        let first = TaskPriority {
            order: 1,
            time: now,
        };
        let second = TaskPriority {
            order: 2,
            time: now,
        };
        let third = TaskPriority {
            order: 0,
            time: later,
        };

        assert_eq!(first.cmp(&second), Ordering::Less);
        assert_eq!(second.cmp(&third), Ordering::Less);
        assert_eq!(first.cmp(&first), Ordering::Equal);
    }

    /// A target time in the past must not be scheduled into the future, and a
    /// bogus one must not panic.
    #[test]
    fn past_and_overflowing_target_times_run_immediately() {
        let before = Instant::now();
        assert!(TaskRunner::flutter_time_to_instant(0) >= before);
        assert!(TaskRunner::flutter_time_to_instant(u64::MAX) >= before);
    }

    /// The queue is shared with the engine's threads through an `Arc`.
    #[test]
    fn the_task_queue_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Task>();
        assert_send_sync::<TaskRunnerInner>();
        assert_send_sync::<TaskRunner>();
    }
}
