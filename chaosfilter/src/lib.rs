//! # ChaosFilter
//!
//! `chaosfilter` is a comprehensive chaos engineering tool for Linux systems, designed to inject
//! controlled faults into specific processes or network interfaces. By leveraging modern Linux
//! kernel features like **eBPF**, **cgroups v2**, and **Traffic Control (tc)**, it provides
//! high-precision failure simulation with minimal overhead.
//!
//! ## Core Capabilities
//!
//! ChaosFilter supports several types of fault injection:
//!
//! - **Network Chaos**: Inject packet loss, delay, and jitter into specific network interfaces
//!   using `tc` and `netem`.
//! - **Resource Constraints**: Apply CPU and memory limits to specific processes using **cgroups v2**.
//! - **System Limits**: Simulate resource exhaustion by lowering file descriptor limits (`RLIMIT_NOFILE`).
//!
//! ## Architecture
//!
//! The project is divided into several crates:
//!
//! - **`chaosfilter`**: The main CLI application (controller). It parses user configurations,
//!   validates plans, and orchestrates the lifecycle of various injectors.
//! - **`chaosfilter-common`**: Shared data structures and constants used by both the controller
//!   and the eBPF programs.
//! - **`chaosfilter-ebpf`**: eBPF programs that run in-kernel to provide low-level monitoring
//!   and filtering capabilities.
//!
//! ## Usage Flow
//!
//! 1. **Plan Definition**: Users define a chaos plan in a TOML file (see [`plans::Plan`]).
//! 2. **Validation**: The controller validates the plan against the system's current state
//!    (see [`validate::validate_plan`]).
//! 3. **Application**: Injectors are activated to apply the requested faults (see [`injector::ChaosInjector`]).
//! 4. **Steady State**: The chaos is maintained for a configured duration.
//! 5. **Reversion**: All changes are reverted, returning the system to its original state.
//!
//! ## Key Modules
//!
//! - [`plans`]: Defines the schema for chaos plans and configuration loading.
//! - [`injector`]: Contains the implementation of various fault injection mechanisms.
//! - [`validate`]: Provides logic to ensure a plan is safe and feasible before execution.

pub mod injector;
pub mod plans;
pub mod validate;