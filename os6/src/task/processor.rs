//! Implementation of [`Processor`] and Intersection of control flow
//!
//! Here, the continuous operation of user apps in CPU is maintained,
//! the current running state of CPU is recorded,
//! and the replacement and transfer of control flow of different applications are executed.


use super::__switch;
use super::{fetch_task, TaskStatus};
use super::{TaskContext, TaskControlBlock};
use crate::sync::UPSafeCell;
use crate::trap::TrapContext;
use alloc::sync::Arc;
use lazy_static::*;
use crate::config::MAX_SYSCALL_NUM;
use crate::mm::{MapPermission,VirtAddr,VirtPageNum};
use crate::mm::address::StepByOne;
use crate::mm::address::VPNRange;
/// Processor management structure
pub struct Processor {
    /// The task currently executing on the current processor
    current: Option<Arc<TaskControlBlock>>,
    /// The basic control flow of each core, helping to select and switch process
    idle_task_cx: TaskContext,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            current: None,
            idle_task_cx: TaskContext::zero_init(),
        }
    }
    fn get_idle_task_cx_ptr(&mut self) -> *mut TaskContext {
        &mut self.idle_task_cx as *mut _
    }
    pub fn take_current(&mut self) -> Option<Arc<TaskControlBlock>> {
        self.current.take()
    }
    pub fn current(&self) -> Option<Arc<TaskControlBlock>> {
        self.current.as_ref().map(|task| Arc::clone(task))
    }
}

lazy_static! {
    /// PROCESSOR instance through lazy_static!
    pub static ref PROCESSOR: UPSafeCell<Processor> = unsafe { UPSafeCell::new(Processor::new()) };
}

/// The main part of process execution and scheduling
///
/// Loop fetch_task to get the process that needs to run,
/// and switch the process through __switch
pub fn run_tasks() {
    loop {
        let mut processor = PROCESSOR.exclusive_access();
        if let Some(task) = fetch_task() {
            let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
            // access coming task TCB exclusively
            let mut task_inner = task.inner_exclusive_access();
            let next_task_cx_ptr = &task_inner.task_cx as *const TaskContext;
            task_inner.task_status = TaskStatus::Running;
            drop(task_inner);
            // release coming task TCB manually
            processor.current = Some(task);
            // release processor manually
            drop(processor);
            unsafe {
                __switch(idle_task_cx_ptr, next_task_cx_ptr);
            }
        }
    }
}

/// Get current task through take, leaving a None in its place
pub fn take_current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().take_current()
}

/// Get a copy of the current task
pub fn current_task() -> Option<Arc<TaskControlBlock>> {
    PROCESSOR.exclusive_access().current()
}

/// Get token of the address space of current task
pub fn current_user_token() -> usize {
    let task = current_task().unwrap();
    let token = task.inner_exclusive_access().get_user_token();
    token
}

/// Get the mutable reference to trap context of current task
pub fn current_trap_cx() -> &'static mut TrapContext {
    current_task()
        .unwrap()
        .inner_exclusive_access()
        .get_trap_cx()
}

/// Return to idle control flow for new scheduling
pub fn schedule(switched_task_cx_ptr: *mut TaskContext) {
    let mut processor = PROCESSOR.exclusive_access();
    let idle_task_cx_ptr = processor.get_idle_task_cx_ptr();
    drop(processor);
    unsafe {
        __switch(switched_task_cx_ptr, idle_task_cx_ptr);
    }
}
pub fn get_current_time() -> usize{
    let task = current_task();
    task.unwrap().inner_exclusive_access().call_time
}
pub fn get_current_num() -> [u32;MAX_SYSCALL_NUM]{
    let task = current_task();
    task.unwrap().inner_exclusive_access().call_num
}
pub fn add_current_num(syscall_id: usize){
    let task = current_task();
    task.unwrap().inner_exclusive_access().call_num[syscall_id]+=1;
}

pub fn mmap_malloc(_start: usize, _len: usize, _port: usize) -> isize{

    if _len ==0{
        return 0;
    }
    if _start%4096 !=0{
        return -1;
    }
    if _port & (!0x7) != 0{
        return -1;
    }
    if _port & 0x7 ==0{
        return -1;
    }
    //let mut inner = self.inner.exclusive_access();
    let binding = current_task().unwrap();
    let mut current = binding.inner_exclusive_access();
    let memory_set = &mut current.memory_set;
    let start: VirtAddr  = VirtAddr(_start).floor().into(); 
    let end_vpn:VirtAddr  = VirtAddr::from(_start+_len).ceil().into();
    if memory_set.check_va_overlap(start.into(), end_vpn.into()){
        return -1;
    }
    let mut permission = MapPermission::from_bits((_port as u8) << 1).unwrap();
    permission.set(MapPermission::U, true);
    memory_set.insert_framed_area(start.into(),end_vpn.into(),permission);
    0

}
pub fn unmap_unalloc(_start: usize, _len: usize) -> isize{
    if _len ==0{
        return 0;
    }
    if _start%4096 !=0{
        return -1;
    }
    //let mut inner = self.inner.exclusive_access();
    let binding = current_task().unwrap();
    let mut current = binding.inner_exclusive_access();
    let memory_set = &mut current.memory_set;
    let mut start = _start; 
    let end = start+_len;
    while start <end{
        let start_va = VirtAddr::from(start);
        let mut vpn = VirtAddr::from(start).floor();
        vpn.step();
        let mut end_va: VirtAddr = vpn.into();
        end_va = end_va.min(VirtAddr::from(end));
        let pte = memory_set.translate(vpn);
        match Some(pte){
            None => return -1,
            _ => (),
        }

        let mut index =0;
        let success =  memory_set.unmap(start_va,end_va);
        if success ==false{
            return -1;
        }
        start = end_va.into();
    }
    0
}