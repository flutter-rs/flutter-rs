use crate::FlutterEngineWeakRef;
use flutter_engine_sys::{FlutterEngineGetCurrentTime, FlutterTask};
use log::debug;
use parking_lot::{Mutex, MutexGuard};
use priority_queue::PriorityQueue;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Weak};
use std::thread;
use std::thread::ThreadId;
use std::time::{Duration, Instant};

pub trait TaskRunnerHandler {
    fn wake(&self);
}

pub(crate) struct TaskRunnerInner {
    engine: FlutterEngineWeakRef,
    pub(crate) handler: Weak<dyn TaskRunnerHandler>,
    thread_id: ThreadId,
    tasks: PriorityQueue<Task, TaskPriority>,
}

pub struct TaskRunner {
    pub(crate) inner: Arc<Mutex<TaskRunnerInner>>,
}

impl Clone for TaskRunner {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl TaskRunnerInner {
    pub(crate) fn runs_task_on_current_thread(&self) -> bool {
        thread::current().id() == self.thread_id
    }
}

impl TaskRunner {
    pub fn new(handler: Weak<dyn TaskRunnerHandler>) -> Self {
        let thread_id = thread::current().id();
        debug!("task runner created on thread {:?}", thread_id);
        Self {
            inner: Arc::new(Mutex::new(TaskRunnerInner {
                engine: Default::default(),
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

    pub fn execute_tasks(&self) -> Option<Instant> {
        let now = Instant::now();
        let mut expired_tasks = Vec::new();

        let engine = {
            let mut inner = self.inner.lock();
            let tasks = &mut inner.tasks;
            while let Some((_, priority)) = tasks.peek() {
                if priority.time > now {
                    break;
                }
                let (task, _) = tasks.pop().unwrap();
                expired_tasks.push(task);
            }
            // make sure to unlock mutex before actually running the tasks as they may post another task
            inner.engine.upgrade().unwrap()
        };

        // run tasks
        for task in expired_tasks {
            engine.run_task(&task.task);
        }

        // next task time
        let mut inner = self.inner.lock();
        let tasks = &mut inner.tasks;
        if let Some((_, priority)) = tasks.peek() {
            Some(priority.time)
        } else {
            None
        }
    }

    fn flutter_time_to_instant(target_time_nanos: u64) -> Instant {
        let current_time = unsafe { FlutterEngineGetCurrentTime() };
        let now = Instant::now();
        if current_time >= target_time_nanos {
            return now;
        }
        let nanos_timeout = target_time_nanos - current_time;
        now.checked_add(Duration::from_nanos(nanos_timeout))
            .unwrap()
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

        let handler = guard.handler.upgrade().unwrap();

        // make sure to unlock the mutex before posting an event because the event handler may read the queue
        MutexGuard::unlocked(guard, move || {
            handler.wake();
        });
    }

    pub(crate) fn runs_task_on_current_thread(&self) -> bool {
        self.inner.lock().runs_task_on_current_thread()
    }

    pub(crate) fn wake(&self) {
        let handler = { self.inner.lock().handler.upgrade() };
        if let Some(handler) = handler {
            handler.wake();
        }
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
