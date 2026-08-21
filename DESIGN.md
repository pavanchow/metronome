# Design

## Cron grammar

Five whitespace-separated fields: minute, hour, day-of-month, month, day-of-week.

```
field      = star | range | step | list
star       = "*"
range      = number "-" number
step       = (star | range | number) "/" number
list       = item ("," item)*
item       = star | range | number
```

Bounds: minute 0-59, hour 0-23, day-of-month 1-31, month 1-12, day-of-week 0-7 (7 is normalized to 0 before parsing, both mean Sunday).

Each field parses down to either "any value in range" or an explicit sorted, deduplicated list of allowed integers. A step with no explicit range (`*/15`) is expanded over the field's full bounds. A step with a range (`1-30/2`) is expanded starting at the range's low bound. Parsing rejects out-of-bounds values, malformed ranges (low > high), and zero steps.

## The next-fire-time algorithm

`next_after(expr, from)` is a pure function: `DateTime -> DateTime`, no side effects, no shared state. It:

1. Truncates `from` to the start of the next whole minute (cron has minute resolution).
2. Walks forward one minute at a time, checking each candidate against all five parsed fields.
3. Returns the first candidate where every field matches.

This is a brute-force minute walk rather than a closed-form jump calculation. It is capped at roughly four years of minutes to avoid an infinite loop on an expression that can never match (for example day-of-month 31 combined with a month that has no 31st day, over and over, though in practice some month always has a 31st). Four years safely covers leap-year cycles and any realistic schedule. The simplicity of the walk is a deliberate trade: it is trivial to verify correct by inspection, and a scheduler that fires at most once a minute has no need for microsecond dispatch performance.

`next_n(expr, from, count)` repeatedly calls `next_after` starting from each previous result, giving the next N fire times.

## Day-of-month vs day-of-week

Standard cron has an easy-to-miss rule: day-of-month and day-of-week are combined with OR, not AND, when both are restricted.

- If both fields are `*`: every day matches.
- If only day-of-month is restricted: the day must match day-of-month; day-of-week is ignored.
- If only day-of-week is restricted: the day must match day-of-week; day-of-month is ignored.
- If both are restricted: the day matches if it satisfies **either** field.

So `0 0 1 * 5` fires at midnight on the 1st of the month, and also every Friday, not only on Fridays that happen to be the 1st.

## Jobs file format

Plain text, one job per line:

```
<cron expr>  <shell command>
```

The cron expression is the first five whitespace-separated fields on the line. Everything after that, split on the first remaining run of whitespace, is the command, passed to `sh -c` verbatim. Blank lines and lines starting with `#` are skipped. A line with a valid cron expression but no command is a parse error, not a silent no-op, so a broken jobs file fails fast at `check` or `run` time instead of silently dropping a job.

## Runtime loop

`run` loads all jobs once, then repeatedly: compute `next_after(now)` for every job, pick the one with the earliest fire time, sleep until then, execute it with `sh -c`, and repeat. This means jobs are evaluated fresh each cycle rather than pre-computing a static schedule, so behavior stays correct across clock adjustments (NTP corrections, DST) without extra bookkeeping.
