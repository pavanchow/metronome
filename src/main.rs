use chrono::Local;
use clap::{Parser, Subcommand};
use metronome::{parse_jobs, CronExpr};
use std::fs;
use std::process::Command as ShellCommand;
use std::thread;
use std::time::Duration as StdDuration;

#[derive(Parser)]
#[command(name = "metronome", version, about = "A single-binary cron-style job scheduler")]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Print the next N fire times for a cron expression
    Next {
        expr: String,
        #[arg(long, default_value_t = 5)]
        count: usize,
    },
    /// Run the scheduler against a jobs file, sleeping until each job is due
    Run {
        #[arg(long)]
        file: String,
    },
    /// Validate a jobs file without running it
    Check {
        #[arg(long)]
        file: String,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Next { expr, count } => cmd_next(&expr, count),
        Cmd::Run { file } => cmd_run(&file),
        Cmd::Check { file } => cmd_check(&file),
    }
}

fn cmd_next(expr: &str, count: usize) {
    let parsed = match CronExpr::parse(expr) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("invalid cron expression: {e}");
            std::process::exit(1);
        }
    };
    let now = Local::now();
    for t in parsed.next_n(&now, count) {
        println!("{}", t.format("%Y-%m-%d %H:%M:%S %a"));
    }
}

fn cmd_check(file: &str) {
    let content = match fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot read {file}: {e}");
            std::process::exit(1);
        }
    };
    match parse_jobs(&content) {
        Ok(jobs) => println!("{} job(s) OK", jobs.len()),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}

fn cmd_run(file: &str) {
    let content = match fs::read_to_string(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("cannot read {file}: {e}");
            std::process::exit(1);
        }
    };
    let jobs = match parse_jobs(&content) {
        Ok(j) => j,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    if jobs.is_empty() {
        eprintln!("no jobs in {file}");
        return;
    }
    println!("metronome: {} job(s) loaded from {file}", jobs.len());

    loop {
        let now = Local::now();
        let mut next_run: Option<(chrono::DateTime<Local>, usize)> = None;
        for (i, job) in jobs.iter().enumerate() {
            if let Some(t) = job.expr.next_after(&now) {
                if next_run.as_ref().map(|(nt, _)| t < *nt).unwrap_or(true) {
                    next_run = Some((t, i));
                }
            }
        }
        let Some((fire_at, idx)) = next_run else {
            eprintln!("no job has a next fire time within the search horizon");
            return;
        };
        let wait = (fire_at - Local::now())
            .to_std()
            .unwrap_or(StdDuration::from_secs(0));
        thread::sleep(wait);

        let job = &jobs[idx];
        println!("metronome: running (line {}) {}", job.line, job.command);
        match ShellCommand::new("sh").arg("-c").arg(&job.command).status() {
            Ok(status) => println!("metronome: exited with {status}"),
            Err(e) => eprintln!("metronome: failed to run job: {e}"),
        }
    }
}
