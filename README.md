# Metronome

**A single-binary cron scheduler in Rust with a schedule engine you can actually unit test.**

System cron is a daemon with global state, root-owned crontabs, and no real way to test a schedule without waiting for it to fire. Metronome takes a different angle: point one static binary at a plain text jobs file and it parses cron expressions, computes the next fire time, and runs jobs in-process. The scheduling logic that decides "when does this fire next" is a pure function in a small library, fully unit tested against known cases, with zero I/O and zero global state. You can embed it in your own Rust project and test your own schedules the same way.

## Why use it

- One binary, no daemon, no system-wide crontab to manage.
- The correctness core, `next_after`, is a pure function: given a cron expression and a point in time, it returns the next fire time. No clock mocking gymnastics needed to test it.
- Jobs live in a plain text file you can diff, review, and check into git.
- Embeddable: pull in the `metronome` library and call `CronExpr::parse` and `next_after` directly from your own code.

## Cron syntax supported

Five fields, in order: minute, hour, day-of-month, month, day-of-week.

| Field | Range | Notes |
|---|---|---|
| minute | 0-59 | |
| hour | 0-23 | |
| day-of-month | 1-31 | |
| month | 1-12 | |
| day-of-week | 0-7 | 0 and 7 both mean Sunday |

Supported syntax per field:

- `*` any value
- `1-5` a range
- `*/15` a step over the full range
- `1-30/2` a step over a range
- `1,3,5` a list of values or ranges

If both day-of-month and day-of-week are restricted (not `*`), a day matches when it satisfies **either** field, matching standard cron semantics. If only one is restricted, only that one applies.

Examples:

- `*/15 * * * *` every 15 minutes
- `0 0 * * *` every day at midnight
- `0 9 * * 1` every Monday at 09:00
- `0 0 1,15 * *` the 1st and 15th of every month at midnight

## Usage

```
# Print the next 5 fire times for an expression
metronome next "*/15 * * * *" --count 5

# Validate a jobs file without running it
metronome check --file jobs.metro

# Run the scheduler: sleeps until each job is due, then runs it
metronome run --file jobs.metro
```

### Jobs file format

One job per line: `<cron expr> <shell command>`. Blank lines and lines starting with `#` are ignored.

```
# jobs.metro
*/15 * * * *  curl -s https://example.com/health > /tmp/health.log
0 3 * * *     /usr/local/bin/backup.sh
0 9 * * 1     echo "start of week"
```

## Building from source

```
cargo build --release
```

Requires `chrono` for time handling and `clap` for the CLI. Everything else, the parser and the scheduling engine, is written from scratch.

## Testing

```
cargo test
```

The test suite covers parser validity/invalidity and `next_after` against known cases: step expressions, daily/weekly schedules, day-of-month vs day-of-week OR semantics, month restrictions, and the `7 == 0` Sunday alias.

See `DESIGN.md` for the grammar and algorithm details.

By Pavan Nallamothu.
