//! Task management implementation
//!
//! Everything about task management, like starting and switching tasks is
//! implemented here.
//!
//! A single global instance of [`TaskManager`] called `TASK_MANAGER` controls
//! all the tasks in the operating system.
//!
//! Be careful when you see [`__switch`]. Control flow around this function
//! might not be what you expect.

mod context;
mod switch;
#[allow(clippy::module_inception)]
mod task;
#[allow(unused_imports)]
use crate::timer::get_time_us;
use crate::config::{MAX_APP_NUM, MAX_SYSCALL_NUM};
use crate::loader::{get_num_app, init_app_cx};
use crate::sync::UPSafeCell;
#[allow(unused_imports)]
use crate::syscall;
use lazy_static::*;
pub use switch::__switch;
pub use task::{TaskControlBlock, TaskStatus};

pub use context::TaskContext;

/// The task manager, where all the tasks are managed.
///
/// Functions implemented on `TaskManager` deals with all task state transitions
/// and task context switching. For convenience, you can find wrappers around it
/// in the module level.
///
/// Most of `TaskManager` are hidden behind the field `inner`, to defer
/// borrowing checks to runtime. You can see examples on how to use `inner` in
/// existing functions on `TaskManager`.
pub struct TaskManager {
    /// total number of tasks
    num_app: usize,
    /// use inner value to get mutable access
    inner: UPSafeCell<TaskManagerInner>,
}

/// The task manager inner in 'UPSafeCell'
struct TaskManagerInner {
    /// task list
    tasks: [TaskControlBlock; MAX_APP_NUM],
    /// id of current `Running` task
    current_task: usize,
}

lazy_static! {
    /// a `TaskManager` instance through lazy_static!
    pub static ref TASK_MANAGER: TaskManager = {
        let num_app = get_num_app();
        let mut tasks = [TaskControlBlock {
            task_cx: TaskContext::zero_init(),
            task_status: TaskStatus::UnInit,
            first_time_task_run: 0,
            task_syscall_record: [0 ; MAX_SYSCALL_NUM],
        }; MAX_APP_NUM];
        for (i, t) in tasks.iter_mut().enumerate().take(num_app) {
            t.task_cx = TaskContext::goto_restore(init_app_cx(i));
            t.task_status = TaskStatus::Ready;
        }
        TaskManager {
            num_app,
            inner: unsafe {
                UPSafeCell::new(TaskManagerInner {
                    tasks,
                    current_task: 0,
                })
            },
        }
    };
}

/* how can we understand trap and __switch 

firstly there is two kinds applications A, B

a trap means that A or B is traping to kernel(S) to finish somthings 

a __switch method is help kernel to change or select which kinds of application or state he want to go 

for example, when there is two kinds of application using syscall to kernel(S) 
and this kind of syscell is a kind of suspend which means the application want cpu just lead it along 
in this kind of situation kernel will just `__switch` to another application

`__switch`:
    current_task_cx_ptr : current task context is in kernel address space
    next_task_cx_ptr    : next app task context in kernel address space

    save each register inside current_task_cx_ptr and recover from next_task_cx_ptr
*/ 




impl TaskManager {
    /// Run the first task in task list.
    ///
    /// Generally, the first task in task list is an idle task (we call it zero process later).
    /// But in ch3, we load apps statically, so the first task is a real app.
    fn run_first_task(&self) -> ! {
        let mut inner = self.inner.exclusive_access();
        let task0 = &mut inner.tasks[0];

        // self.record_current_first_run_time(0);          // BC we start from the first one 
        if task0.first_time_task_run != 0 {
            task0.first_time_task_run = get_time_us();
        }

        task0.task_status = TaskStatus::Running;
        let next_task_cx_ptr = &task0.task_cx as *const TaskContext;
        drop(inner);
        let mut _unused = TaskContext::zero_init();
        // before this, we should drop local variables that must be dropped manually
        unsafe {
            __switch(&mut _unused as *mut TaskContext, next_task_cx_ptr);
        }
        panic!("unreachable in run_first_task!");
    }

    /// Change the status of current `Running` task into `Ready`.
    fn mark_current_suspended(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Ready;
    }

    /// Change the status of current `Running` task into `Exited`.
    fn mark_current_exited(&self) {
        let mut inner = self.inner.exclusive_access();
        let current = inner.current_task;
        inner.tasks[current].task_status = TaskStatus::Exited;
    }

    /// Find next task to run and return task id.
    ///
    /// In this case, we only return the first `Ready` task in task list.
    fn find_next_task(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        let current = inner.current_task;
        (current + 1..current + self.num_app + 1)
            .map(|id| id % self.num_app)
            .find(|id| inner.tasks[*id].task_status == TaskStatus::Ready)
    }

    /// Switch current `Running` task to the task we have found,
    /// or there is no `Ready` task and we can exit with all applications completed
    fn run_next_task(&self) {
        if let Some(next) = self.find_next_task() {
            let mut inner = self.inner.exclusive_access();
            let current = inner.current_task;
            // self.record_current_first_run_time(next);
            inner.tasks[next].task_status = TaskStatus::Running;
            inner.current_task = next;

            if inner.tasks[next].first_time_task_run != 0 {
                inner.tasks[next].first_time_task_run = get_time_us();
            }

            let current_task_cx_ptr = &mut inner.tasks[current].task_cx as *mut TaskContext;
            let next_task_cx_ptr = &inner.tasks[next].task_cx as *const TaskContext;
            drop(inner);
            // before this, we should drop local variables that must be dropped manually
            unsafe {
                __switch(current_task_cx_ptr, next_task_cx_ptr);
            }
            // go back to user mode
        } else {
            panic!("All applications completed!");
        }
    }

    // TODO LAB1: Try to implement your function to update or get task info!

    /*
    BC we need to show this kinds of information :
        TaskStatus:     [FINISH]
        system_time: 
            the syscall a application has use 
            we try to put a public function in task [we will call it in syscall mod]
        time: 
            the gap between this time and the first time application has been use  
            because we need to get the time diff between `sys_task_info` and the first time this task been use
            so, in my view :
                we could just record the first time this task has been use.
                and we account this information when we using `sys_task_info` 
    */


    /// **Because Iteration borrow** we should using it here [ try to embed it in code ]
    // #[allow(unused_variables)]
    // #[allow(dead_code)]
    // pub fn record_current_first_run_time(&self, task_id:usize) {
    //     let mut inner = self.inner.exclusive_access();
    //     let current_task = inner.current_task;
        
    //     if inner.tasks[current_task].first_time_task_run != 0 {
    //         inner.tasks[current_task].first_time_task_run = get_time();
    //     }
    // }

    #[allow(dead_code)]
    pub fn add_current_system_call_record(&self, id:usize) {
        let mut inner = self.inner.exclusive_access();
        // inner.tasks[inner.current_task].task_syscall_record[id] += 1;
        // BC inner.current is immutable and inner.tasks is mutable, this two kinds of things couldn't exist at the same time
        // we try to using `sql nested update` to attach this condition
        let current_task = inner.current_task;
        inner.tasks[current_task].task_syscall_record[id] += 1;
        if  id == 64 || id == 93 {
            println!("task: {} using syscall: {} now it's {}", current_task, id, inner.tasks[current_task].task_syscall_record[id]);
        }
    }

    // getting information
    #[allow(dead_code)]
    pub fn get_current_task_status(&self) -> Option<TaskStatus> {
        let inner = self.inner.exclusive_access();
        Some(inner.tasks[inner.current_task].task_status)   // BC we only use Immutable reference so we could using it here 
    }

    #[allow(dead_code)]
    pub fn get_current_syscall_record(&self) -> Option<[u32 ; MAX_SYSCALL_NUM]> {
        let inner = self.inner.exclusive_access();
        Some(inner.tasks[inner.current_task].task_syscall_record)
    }

    #[allow(dead_code)]
    pub fn get_current_task_first_run_time(&self) -> Option<usize> {
        let inner = self.inner.exclusive_access();
        Some(inner.tasks[inner.current_task].first_time_task_run)
    }

    // BUG
    // pub fn get_task_id(&self) -> usize{
    //     let inner = self.inner.exclusive_access();
    //     inner.current_task
    // }
}

/// Run the first task in task list.
pub fn run_first_task() {
    TASK_MANAGER.run_first_task();
}

/// Switch current `Running` task to the task we have found,
/// or there is no `Ready` task and we can exit with all applications completed
fn run_next_task() {
    TASK_MANAGER.run_next_task();
}

/// Change the status of current `Running` task into `Ready`.
fn mark_current_suspended() {
    TASK_MANAGER.mark_current_suspended();
}

/// Change the status of current `Running` task into `Exited`.
fn mark_current_exited() {
    TASK_MANAGER.mark_current_exited();
}

/// Suspend the current 'Running' task and run the next task in task list.
pub fn suspend_current_and_run_next() {
    mark_current_suspended();
    run_next_task();
}

/// Exit the current 'Running' task and run the next task in task list.
pub fn exit_current_and_run_next() {
    mark_current_exited();
    run_next_task();
}

// TODO LAB1: Public functions implemented here provide interfaces.
// You may use TASK_MANAGER member functions to handle requests.
#[allow(dead_code)]
pub fn record_system_call(id : usize) {
    TASK_MANAGER.add_current_system_call_record(id);
}

#[allow(dead_code)]
pub fn get_current_task_info() -> (TaskStatus, [u32; MAX_SYSCALL_NUM], usize) {
    // maybe we give a kind of mutable variable is un improper
    // let mut ret: (TaskStatus, [u32;MAX_SYSCALL_NUM], usize) = (TaskStatus::UnInit, [0; MAX_SYSCALL_NUM], 0);
    
    // if let Some(status) = TASK_MANAGER.get_current_task_status() {
    //     ret.0 = status;
    // }
    // if let Some(syscall_record) = TASK_MANAGER.get_current_syscall_record() {
    //     ret.1 = syscall_record;
    // }
    // if let Some(first_run_time) = TASK_MANAGER.get_current_task_first_run_time() {
    //     ret.2 = first_run_time;
    // }

    // ret

    let (status, syscall_record, first_run_time) : (TaskStatus, [u32; MAX_SYSCALL_NUM], usize);
    status = match TASK_MANAGER.get_current_task_status() {
        Some(status) => status,
        _ => TaskStatus::UnInit
    };
    syscall_record = match TASK_MANAGER.get_current_syscall_record() {
        Some(syscall_record) => syscall_record,
        _ => [0; MAX_SYSCALL_NUM],
    };

    // BC we only record the first run time of this programe so we just using the time right now to sub it 
    first_run_time = match TASK_MANAGER.get_current_task_first_run_time() {
        Some(first_run_time) => (get_time_us() - first_run_time) / 1000,
        _ => 0,
    };
    (status, syscall_record, first_run_time)
}

// #[debug]
// pub fn print_taks_id(&) {
    // if TASK_MANAGER.get_task_id() != None {
        // println!("Current Task: {} ", TASK_MANAGER.get_task_id());
    // }
// }