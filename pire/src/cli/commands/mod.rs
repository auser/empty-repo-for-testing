use clap::Subcommand;

pub mod doctor;

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Diagnose configuration and startup issues.
    Doctor(doctor::DoctorArgs),
}
