//! F-only component witness. Calls the shipping sim-core APIs, not copied algorithms.
//! The two cfg fault modes alter ONLY this declared test driver's control flow.
//! This is not a complete BHAV/lot/scheduler/reference-engine execution.
use sim_core::clock::SimClock;
use sim_core::rng::SimRng;
use std::fmt::Write;
use std::sync::OnceLock;

const CASES: &str = include_str!("../../fixtures/reference/clock-rng-cases.tsv");
const CASE_HEADER: &str = "case\tts1\tseed\tutc_start\tstart_tick\tfraction\tminutes\thours\tday\tmonth\tyear\tfire\tsteps";
const HEADER: &str = "case\tstep\tclock_tick\trng_before\tbound\trandom\tbranch\textra\trng_after\tfraction\tminutes\thours\tday\tmonth\tyear\tfire\tseconds\tutc\n";
const BOUNDS: [u64; 6] = [0, 1, 2, 100, 65535, u64::MAX];
static OUTPUT: OnceLock<String> = OnceLock::new();

fn run() -> String {
    let mut lines = CASES.lines();
    assert_eq!(lines.next(), Some(CASE_HEADER));
    let mut output = String::from(HEADER);
    let mut cases = 0;
    let mut rows = 0;
    for line in lines {
        let values: Vec<&str> = line.split('\t').collect();
        assert_eq!(values.len(), 13);
        cases += 1;
        assert!(cases <= 64);
        let signed = |i: usize| values[i].parse::<i32>().expect("verified i32 fixture");
        assert!(values[1] == "0" || values[1] == "1");
        let mut clock = SimClock::new(values[1] == "1", values[3].parse().unwrap());
        clock.ticks = values[4].parse().unwrap();
        clock.minute_fractions = signed(5);
        clock.minutes = signed(6);
        clock.hours = signed(7);
        clock.day_of_month = signed(8);
        clock.month = signed(9);
        clock.year = signed(10);
        clock.fire_percent = signed(11);
        clock.validate().expect("valid declared clock");
        let mut rng = SimRng::new(values[2].parse().unwrap());
        let steps = values[12].parse::<usize>().unwrap();
        assert!((1..=2048).contains(&steps));
        for step in 1..=steps {
            rows += 1;
            assert!(rows <= 10000);
            let before = rng.state();
            let inject = values[0] == "ts1_rollover" && step == 7;
            if cfg!(reference_fault_rng) && inject {
                let _ = rng.next(1); // Negative control: one erroneous real RNG consumption.
            }
            let bound = BOUNDS[(step - 1) % BOUNDS.len()];
            let value = rng.next(bound);
            let mut branch = (value & 1) != 0;
            if cfg!(reference_fault_branch) && inject {
                branch = !branch; // Negative control: wrong branch in the test driver.
            }
            let extra = rng.next(if branch { 17 } else { 0 });
            clock.advance().expect("declared clock remains in range");
            writeln!(
                output,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                values[0], step, clock.ticks, before, bound, value, u8::from(branch), extra,
                rng.state(), clock.minute_fractions, clock.minutes, clock.hours,
                clock.day_of_month, clock.month, clock.year, clock.fire_percent,
                clock.seconds().unwrap(), clock.utc_dotnet_ticks().unwrap(),
            ).unwrap();
        }
    }
    assert!(cases > 0);
    output
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    print!("{}", OUTPUT.get_or_init(run));
}

// SAFETY: these unique probe-only exports return an immutable process-lifetime
// buffer. No caller-supplied pointer is accepted or dereferenced. The host cannot
// free the buffer; repeated reads do not rerun the scenario or advance its state.
#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn reference_ptr() -> *const u8 {
    OUTPUT.get_or_init(run).as_ptr()
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn reference_len() -> usize {
    OUTPUT.get_or_init(run).len()
}
