//! Timing and hardware performance counters.
//!
//! [`Counters`] opens one `perf_event_open` counter per [`Event`] (Linux). Any counter the
//! kernel, VM or CPU refuses reads as NaN instead of failing the measurement, counters the kernel
//! had to multiplex are scaled (like `perf stat`), and every [`Sample`] also carries wall-clock
//! time and fixed-rate counter ticks ([`fixed_counter`]), so cycles-per-op is available even
//! without a PMU.

use std::time::Instant;

/// Fixed-rate counter: TSC on x86-64, `cntvct_el0` on AArch64, nanoseconds elsewhere. Ticks at a
/// constant rate, not the core clock (see [`core_clock`]).
#[inline(always)]
pub fn fixed_counter() -> u64 {
    #[cfg(target_arch = "x86_64")]
    // SAFETY: rdtsc is available on every x86-64 CPU.
    unsafe {
        core::arch::x86_64::_rdtsc()
    }
    #[cfg(target_arch = "aarch64")]
    {
        let v: u64;
        // SAFETY: cntvct_el0 is readable from EL0 on Linux.
        unsafe { core::arch::asm!("mrs {0}, cntvct_el0", out(reg) v, options(nomem, nostack, preserves_flags)) };
        v
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    {
        use std::sync::OnceLock;
        static T0: OnceLock<Instant> = OnceLock::new();
        T0.get_or_init(Instant::now).elapsed().as_nanos() as u64
    }
}

/// Hardware events, named as `perf list` names them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Event {
    Cycles,
    Instructions,
    Branches,
    BranchMisses,
    /// `L1-dcache-load-misses`
    L1dReadMisses,
    /// `LLC-load-misses` (not available on every CPU, e.g. AMD Zen)
    LlcReadMisses,
    /// `cache-misses`: the generic last-level-cache-miss event
    CacheMisses,
}

impl Event {
    pub const ALL: [Event; 7] = [
        Event::Cycles,
        Event::Instructions,
        Event::Branches,
        Event::BranchMisses,
        Event::L1dReadMisses,
        Event::LlcReadMisses,
        Event::CacheMisses,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Event::Cycles => "cycles",
            Event::Instructions => "instructions",
            Event::Branches => "branches",
            Event::BranchMisses => "branch-misses",
            Event::L1dReadMisses => "L1-dcache-load-misses",
            Event::LlcReadMisses => "LLC-load-misses",
            Event::CacheMisses => "cache-misses",
        }
    }

    fn index(self) -> usize {
        Event::ALL.iter().position(|&e| e == self).unwrap()
    }
}

/// One measurement: wall time, fixed-counter ticks and the counted events.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub ns: f64,
    /// [`fixed_counter`] ticks.
    pub ticks: f64,
    values: [f64; Event::ALL.len()],
    /// Some counter was multiplexed (and scaled), so its value is an estimate.
    pub multiplexed: bool,
}

impl Sample {
    /// All zeros: the start of a sum (events that are not counted stay NaN once a measured sample
    /// is added).
    pub const ZERO: Sample = Sample { ns: 0.0, ticks: 0.0, values: [0.0; Event::ALL.len()], multiplexed: false };

    /// The event's count, or NaN if it was not counted.
    pub fn get(&self, e: Event) -> f64 {
        self.values[e.index()]
    }

    /// Core cycles if the PMU counted them, else fixed-counter ticks; the flag says which.
    pub fn cycles(&self) -> (f64, CycleSource) {
        let c = self.get(Event::Cycles);
        if c.is_nan() { (self.ticks, CycleSource::FixedCounter) } else { (c, CycleSource::Pmu) }
    }

    /// Instructions per core cycle, or NaN without both counters.
    pub fn ipc(&self) -> f64 {
        self.get(Event::Instructions) / self.get(Event::Cycles)
    }

    /// Every value divided by `n` (e.g. per repetition).
    pub fn per(&self, n: f64) -> Sample {
        let mut s = *self;
        s.ns /= n;
        s.ticks /= n;
        s.values.iter_mut().for_each(|v| *v /= n);
        s
    }
}

impl std::ops::AddAssign<&Sample> for Sample {
    fn add_assign(&mut self, o: &Sample) {
        self.ns += o.ns;
        self.ticks += o.ticks;
        for (a, b) in self.values.iter_mut().zip(o.values) {
            *a += b;
        }
        self.multiplexed |= o.multiplexed;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CycleSource {
    /// Core cycles from the PMU.
    Pmu,
    /// [`fixed_counter`] ticks (TSC): a fixed rate, not core cycles.
    FixedCounter,
}

impl CycleSource {
    pub fn name(self) -> &'static str {
        match self {
            CycleSource::Pmu => "hw",
            CycleSource::FixedCounter => "tsc",
        }
    }
}

struct Counter {
    event: Event,
    fd: i32,
}

/// A set of hardware counters for the calling thread.
pub struct Counters {
    counters: Vec<Counter>,
}

#[cfg(target_os = "linux")]
mod sys {
    pub const PERF_TYPE_HARDWARE: u32 = 0;
    pub const PERF_TYPE_HW_CACHE: u32 = 3;
    pub const PERF_COUNT_HW_CPU_CYCLES: u64 = 0;
    pub const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
    pub const PERF_COUNT_HW_CACHE_MISSES: u64 = 3;
    pub const PERF_COUNT_HW_BRANCH_INSTRUCTIONS: u64 = 4;
    pub const PERF_COUNT_HW_BRANCH_MISSES: u64 = 5;
    pub const PERF_COUNT_HW_CACHE_L1D: u64 = 0;
    pub const PERF_COUNT_HW_CACHE_LL: u64 = 2;
    pub const PERF_COUNT_HW_CACHE_OP_READ: u64 = 0;
    pub const PERF_COUNT_HW_CACHE_RESULT_MISS: u64 = 1;
    pub const PERF_FORMAT_TOTAL_TIME_ENABLED: u64 = 1 << 0;
    pub const PERF_FORMAT_TOTAL_TIME_RUNNING: u64 = 1 << 1;
    pub const PERF_EVENT_IOC_ENABLE: libc::c_ulong = 0x2400;
    pub const PERF_EVENT_IOC_DISABLE: libc::c_ulong = 0x2401;
    pub const PERF_EVENT_IOC_RESET: libc::c_ulong = 0x2403;

    /// `struct perf_event_attr` (PERF_ATTR_SIZE_VER8, 136 bytes).
    #[repr(C)]
    #[derive(Default)]
    pub struct Attr {
        pub type_: u32,
        pub size: u32,
        pub config: u64,
        pub sample: u64,
        pub sample_type: u64,
        pub read_format: u64,
        pub flags: u64,
        pub wakeup: u32,
        pub bp_type: u32,
        pub bp_addr: u64,
        pub bp_len: u64,
        pub branch_sample_type: u64,
        pub sample_regs_user: u64,
        pub sample_stack_user: u32,
        pub clockid: i32,
        pub sample_regs_intr: u64,
        pub aux_watermark: u32,
        pub sample_max_stack: u16,
        pub reserved: u16,
        pub aux_sample_size: u32,
        pub reserved2: u32,
        pub sig_data: u64,
        pub config3: u64,
    }
    const _: () = assert!(size_of::<Attr>() == 136);

    pub fn spec(e: super::Event) -> (u32, u64) {
        use super::Event::*;
        let cache = |c: u64| c | (PERF_COUNT_HW_CACHE_OP_READ << 8) | (PERF_COUNT_HW_CACHE_RESULT_MISS << 16);
        match e {
            Cycles => (PERF_TYPE_HARDWARE, PERF_COUNT_HW_CPU_CYCLES),
            Instructions => (PERF_TYPE_HARDWARE, PERF_COUNT_HW_INSTRUCTIONS),
            Branches => (PERF_TYPE_HARDWARE, PERF_COUNT_HW_BRANCH_INSTRUCTIONS),
            BranchMisses => (PERF_TYPE_HARDWARE, PERF_COUNT_HW_BRANCH_MISSES),
            L1dReadMisses => (PERF_TYPE_HW_CACHE, cache(PERF_COUNT_HW_CACHE_L1D)),
            LlcReadMisses => (PERF_TYPE_HW_CACHE, cache(PERF_COUNT_HW_CACHE_LL)),
            CacheMisses => (PERF_TYPE_HARDWARE, PERF_COUNT_HW_CACHE_MISSES),
        }
    }
}

impl Counters {
    /// The usual set: cycles, instructions, branch misses, L1D and LLC read misses.
    pub fn standard() -> Self {
        Self::new(&[Event::Cycles, Event::Instructions, Event::BranchMisses, Event::L1dReadMisses, Event::LlcReadMisses])
    }

    pub fn new(events: &[Event]) -> Self {
        let counters = events.iter().map(|&event| Counter { event, fd: Self::open(event) }).collect();
        Counters { counters }
    }

    #[cfg(target_os = "linux")]
    fn open(event: Event) -> i32 {
        use sys::*;
        let (type_, config) = spec(event);
        let a = Attr {
            type_,
            size: size_of::<Attr>() as u32,
            config,
            read_format: PERF_FORMAT_TOTAL_TIME_ENABLED | PERF_FORMAT_TOTAL_TIME_RUNNING,
            flags: 1 | (1 << 5) | (1 << 6), // disabled, exclude_kernel, exclude_hv
            ..Default::default()
        };
        // SAFETY: a valid perf_event_attr; pid 0 / cpu -1 = this thread on any CPU, no group.
        unsafe { libc::syscall(libc::SYS_perf_event_open, &a as *const Attr, 0i32, -1i32, -1i32, 0u64) as i32 }
    }
    #[cfg(not(target_os = "linux"))]
    fn open(_: Event) -> i32 {
        -1
    }

    /// Whether any counter could be opened.
    pub fn any_available(&self) -> bool {
        self.counters.iter().any(|c| c.fd >= 0)
    }

    /// `name:ok name:n/a ...`
    pub fn status(&self) -> String {
        let s: Vec<String> =
            self.counters.iter().map(|c| format!("{}:{}", c.event.name(), if c.fd >= 0 { "ok" } else { "n/a" })).collect();
        s.join(" ")
    }

    /// Counts `body`.
    pub fn measure(&self, body: impl FnOnce()) -> Sample {
        self.ioctl_all(true);
        let t0 = fixed_counter();
        let w0 = Instant::now();
        body();
        let ns = w0.elapsed().as_nanos() as f64;
        let t1 = fixed_counter();
        self.ioctl_all(false);
        let mut s = Sample { ns, ticks: t1.wrapping_sub(t0) as f64, values: [f64::NAN; Event::ALL.len()], multiplexed: false };
        for c in &self.counters {
            if let Some((v, scaled)) = Self::read(c.fd) {
                s.values[c.event.index()] = v;
                s.multiplexed |= scaled;
            }
        }
        s
    }

    #[cfg(target_os = "linux")]
    fn ioctl_all(&self, enable: bool) {
        for c in self.counters.iter().filter(|c| c.fd >= 0) {
            // SAFETY: fd is an open perf event.
            unsafe {
                if enable {
                    libc::ioctl(c.fd, sys::PERF_EVENT_IOC_RESET, 0);
                    libc::ioctl(c.fd, sys::PERF_EVENT_IOC_ENABLE, 0);
                } else {
                    libc::ioctl(c.fd, sys::PERF_EVENT_IOC_DISABLE, 0);
                }
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    fn ioctl_all(&self, _: bool) {}

    /// `(value, was multiplexed)`, scaled by enabled/running time; None if the event never ran.
    #[cfg(target_os = "linux")]
    fn read(fd: i32) -> Option<(f64, bool)> {
        if fd < 0 {
            return None;
        }
        let mut buf = [0u64; 3]; // value, time_enabled, time_running
        // SAFETY: fd is an open perf event; reading 24 bytes into buf.
        let n = unsafe { libc::read(fd, buf.as_mut_ptr().cast(), size_of_val(&buf)) };
        let [value, enabled, running] = buf;
        if n != size_of_val(&buf) as isize || running == 0 {
            return None;
        }
        if running < enabled { Some((value as f64 * enabled as f64 / running as f64, true)) } else { Some((value as f64, false)) }
    }
    #[cfg(not(target_os = "linux"))]
    fn read(_: i32) -> Option<(f64, bool)> {
        None
    }
}

impl Drop for Counters {
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        for c in self.counters.iter().filter(|c| c.fd >= 0) {
            // SAFETY: fd is an open perf event owned by self.
            unsafe { libc::close(c.fd) };
        }
    }
}

/// Core clock relative to the fixed counter, from [`core_clock`].
#[derive(Clone, Copy, Debug)]
pub struct CoreClock {
    pub tsc_ghz: f64,
    pub core_per_tsc: f64,
}

impl CoreClock {
    pub fn core_ghz(&self) -> f64 {
        self.core_per_tsc * self.tsc_ghz
    }
}

/// Estimates the core clock relative to the TSC with a dependent `imul` chain (3-cycle latency
/// on every Intel/AMD core since Sandy Bridge/Zen): core cycles = TSC ticks x `core_per_tsc`.
/// Takes a few seconds; the best of `trials` runs is kept. `None` off x86-64.
pub fn core_clock(trials: usize) -> Option<CoreClock> {
    #[cfg(target_arch = "x86_64")]
    {
        let mut best = CoreClock { tsc_ghz: 0.0, core_per_tsc: 0.0 };
        for _ in 0..trials {
            let n = 300_000_000u64;
            let mut x = 3u64;
            let t0 = Instant::now();
            let c0 = fixed_counter();
            for _ in 0..n {
                // 4 dependent imuls = 12 core cycles per iteration
                // SAFETY: plain register arithmetic.
                unsafe {
                    core::arch::asm!("imul {x}, {y}", "imul {x}, {y}", "imul {x}, {y}", "imul {x}, {y}", x = inout(reg) x, y = in(reg) x | 1, options(nomem, nostack))
                };
            }
            let c1 = fixed_counter();
            let ns = t0.elapsed().as_nanos() as f64;
            std::hint::black_box(x);
            let ratio = 12.0 / ((c1 - c0) as f64 / n as f64);
            if ratio > best.core_per_tsc {
                best = CoreClock { tsc_ghz: (c1 - c0) as f64 / ns, core_per_tsc: ratio };
            }
        }
        Some(best)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = trials;
        None
    }
}
