
#[cfg(target_os = "linux")]
extern crate libc;

#[cfg(target_os = "linux")]
pub mod linux {
    use std::mem;
 
    use libc::{CPU_ISSET, CPU_SET, CPU_SETSIZE, cpu_set_t, sched_getaffinity, sched_setaffinity};

    pub fn set_affinity_mask(mask: usize) {
        let mut set = new_cpu_set();

        unsafe { CPU_SET(core_id.id, &mut set) };

        // Set the current thread's core affinity.
        unsafe {
            sched_setaffinity(0, // Defaults to current thread
                              mem::size_of::<cpu_set_t>(),
                              &set);
        }
    }

    pub fn get_affinity_mask() -> Option<cpu_set_t> {
        let mut set = new_cpu_set();

        // Try to get current core affinity mask.
        let result = unsafe {
            sched_getaffinity(0, // Defaults to current thread
                              mem::size_of::<cpu_set_t>(),
                              &mut set)
        };

        if result == 0 {
            Some(set)
        }
        else {
            None
        }
    }
}

#[cfg(target_os = "windows")]
extern crate winapi;

#[cfg(target_os = "windows")]
pub mod windows {
    use winapi::shared::basetsd::{DWORD_PTR, PDWORD_PTR};

    pub fn set_affinity_mask(mask: usize) {

        // Set core affinity for current thread.
        unsafe {
            winapi::um::winbase::SetThreadAffinityMask(
                winapi::um::processthreadsapi::GetCurrentThread(),
                mask as DWORD_PTR
            );
        }
    }

    pub fn get_affinity_mask() -> Option<usize> {
        let mut process_mask: usize = 0;
        let mut system_mask: usize = 0;

        let res = unsafe {
            winapi::um::winbase::GetProcessAffinityMask(
                winapi::um::processthreadsapi::GetCurrentProcess(),
                &mut process_mask as PDWORD_PTR,
                &mut system_mask as PDWORD_PTR
            )
        };
        
        // Successfully retrieved affinity mask
        if res != 0 {
            Some(process_mask as usize)
        }
        // Failed to retrieve affinity mask
        else {
            None
        }
    }
}