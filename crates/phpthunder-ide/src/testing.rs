//! Test runner: Pest, PHPUnit, Codeception, Behat or PHPSpec from
//! `vendor/bin`, results from JUnit XML and coverage from Clover XML.

mod coverage;
mod junit;
mod runner;

pub use coverage::*;
pub use junit::*;
pub use runner::*;
