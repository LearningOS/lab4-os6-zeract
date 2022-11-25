//! Implementation of [`TaskManager`]
//!
//! It is only used to manage processes and schedule process based on ready queue.
//! Other CPU process monitoring functions are in Processor.


use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;

pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

// YOUR JOB: FIFO->Stride
/// A simple FIFO scheduler.
impl TaskManager {
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        let len = self.ready_queue.len();
        let mut min_pass:u32= u32::MAX;
        let mut index = 0;
        for i in 0..=len-1{
            let element = self.ready_queue.get(i);
            let task = element.unwrap();
            if i==0{
                min_pass = task.inner_exclusive_access().pass;
                index = i;
            }
            else{
                let diff:i32 = (task.inner_exclusive_access().pass - min_pass) as i32;
                if  diff<0{
                    index = i;
                    min_pass = element.unwrap().inner_exclusive_access().pass;
                }
            }

        }
        let ele = self.ready_queue.get(index);
        ele.unwrap().inner_exclusive_access().add_pass();
        self.ready_queue.remove(index)
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

pub fn add_task(task: Arc<TaskControlBlock>) {
    TASK_MANAGER.exclusive_access().add(task);
}

pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    TASK_MANAGER.exclusive_access().fetch()
}
