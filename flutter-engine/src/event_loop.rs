use std::{
    cmp::{Ord, Ordering, PartialOrd},
    hash::{Hash, Hasher},
    sync::{atomic::AtomicU64, Arc, Mutex},
    thread::{self, ThreadId},
    time::{Duration, Instant},
};

use glfw::Glfw;
use log::debug;
use priority_queue::PriorityQueue;

use flutter_engine_sys::{FlutterEngineGetCurrentTime, FlutterTask};

use crate::ffi::FlutterEngine;

pub struct EventLoop {
    thread_id: ThreadId,
    glfw: Glfw,
    tasks: Mutex<PriorityQueue<Task, TaskPriority>>,
}

#[derive(Eq, PartialEq)]
struct TaskPriority {
    order: u64,
    time: Instant,
}

struct Task {
    task: FlutterTask,
}

impl EventLoop {
    pub fn new(glfw: Glfw) -> Self {
        let thread_id = thread::current().id();
        debug!("event loop running in thread {:?}", thread_id);
        Self {
            thread_id,
            glfw,
            tasks: Mutex::new(PriorityQueue::new()),
        }
    }

    pub fn runs_task_on_current_thread(&self) -> bool {
        thread::current().id() == self.thread_id
    }

    pub fn wait_for_events(&mut self, engine: &Arc<FlutterEngine>) {
        let now = Instant::now();
        let mut expired_tasks = Vec::new();

        {
            let mut tasks = self.tasks.lock().unwrap();
            while let Some((_, priority)) = tasks.peek() {
                if priority.time > now {
                    break;
                }
                let (task, _) = tasks.pop().unwrap();
                expired_tasks.push(task);
            }
            // make sure to unlock mutex before actually running the tasks as they may post another task
        }

        // run tasks
        for task in expired_tasks {
            engine.run_task(&task.task);
        }

        let next_task_time = {
            let tasks = self.tasks.lock().unwrap();
            if let Some((_, priority)) = tasks.peek() {
                Some(priority.time)
            } else {
                None
            }
        };

        if let Some(next_task_time) = next_task_time {
            let now = Instant::now();
            if now < next_task_time {
                let duration = next_task_time.duration_since(now);
                let secs = duration.as_secs() as f64 + duration.subsec_nanos() as f64 * 1e-9;
                self.glfw.wait_events_timeout(secs);
            } else {
                self.glfw.poll_events();
            }
        } else {
            self.glfw.wait_events();
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

    pub fn post_task(&mut self, task: FlutterTask, target_time_nanos: u64) {
        static GLOBAL_ORDER: AtomicU64 = AtomicU64::new(0);
        let task_priority = TaskPriority {
            time: Self::flutter_time_to_instant(target_time_nanos),
            order: GLOBAL_ORDER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        };
        let task = Task { task };

        {
            let mut tasks = self.tasks.lock().unwrap();
            tasks.push(task, task_priority);

            // make sure to unlock the mutex before posting an event because the event handler may read the queue
        }

        self.glfw.post_empty_event();
    }
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
