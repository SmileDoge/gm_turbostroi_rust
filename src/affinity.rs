
#[cfg(target_os = "linux")]
extern crate libc;

#[cfg(target_os = "linux")]
pub mod linux {
    use std::mem;
 
    use libc::{CPU_ISSET, CPU_SET, cpu_set_t, sched_getaffinity, sched_setaffinity};

    pub fn set_affinity_mask(mask: usize) {
        let mut set = new_cpu_set();

        for i in 0..32 {
            if ((1 << i) & mask) > 0 {
                unsafe { CPU_SET(i, &mut set) };
            }
        }

        // Set the current thread's core affinity.
        unsafe {
            sched_setaffinity(0, // Defaults to current thread
                              mem::size_of::<cpu_set_t>(),
                              &set);
        }
    }

    pub fn get_affinity_mask() -> Option<usize> {
        let mut set = new_cpu_set();

        // Try to get current core affinity mask.
        let result = unsafe {
            sched_getaffinity(0, // Defaults to current thread
                              mem::size_of::<cpu_set_t>(),
                              &mut set)
        };

        if result == 0 {
            let mut bits: usize = 0;
        
            unsafe{
                for i in 0..32 {
                    if CPU_ISSET(i, &mut set) {
                        bits = bits | 1 << i;
                    }
                }
            }

            Some(bits)
        }
        else {
            None
        }
    }

    fn new_cpu_set() -> cpu_set_t {
        unsafe { mem::zeroed::<cpu_set_t>() }
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