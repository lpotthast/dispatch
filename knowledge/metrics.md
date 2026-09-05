---
id: dispatch.metrics
refines:
  - dispatch.architecture
---

# Backend Metrics

Dispatch captures backend performance metrics so slow workflow paths can be separated into repository work, database
execution, and client transport or rendering costs. Metrics are operational observations, not persistent product data.

## Architecture

The server uses the `metrics` facade as the instrumentation boundary. Startup installs one Dispatch-owned recorder
before opening the database, so migrations, repository operations, and later requests use the same capture path.
Business and repository code emits measurements through the facade and does not depend on the recorder or the UI
representation.

The recorder aggregates in memory with thread-safe counters, gauges, and fixed-bucket histograms. A snapshot converts
those internal series into shared typed DTOs for the frontend. Metric state is cumulative for the life of the server
process, is intentionally not written to SQLite, and resets when the server restarts. This keeps measurement out of the
domain schema and prevents observability from becoming a database workload of its own.

## Repository timing

Meaningful backend boundaries use the shared repository timing wrapper. It records
`dispatch_repository_operation_duration_seconds` with stable `operation` and `outcome` labels. Operations should
describe reusable backend work such as `board.page`, `work_items.list`, or `work_items.enrich`; they must not contain
project names, item IDs, error messages, or other unbounded values.

Nested timings are intentional when they answer different questions. For example, the board page total identifies
user-visible backend time while item listing, enrichment, and run-preview timings expose expensive phases inside it. Do
not add timers around every helper. Instrument a boundary only when its duration is operationally meaningful and its
label has bounded cardinality.

## SQL timing

SQL timing is automatic. SQLx emits structured `sqlx::query` tracing events and the server tracing subscriber includes a
metrics layer that reads their elapsed time, normalized statement summary, returned-row count, and affected-row count.
It records:

- `dispatch_sql_query_duration_seconds` as a histogram;
- `dispatch_sql_queries_total` as a counter;
- `dispatch_sql_rows_returned_total` as a counter;
- `dispatch_sql_rows_affected_total` as a counter.

The statement label is SQLx's normalized summary, shortened to a fixed maximum length. Bound parameter values are not
captured. This provides query-family detail without exposing request data or allowing arbitrary SQL text to create
unbounded metric series. Repository timings include connection-pool waits, mapping, validation, and other application
work, whereas SQL timings describe the database execution reported by SQLx; the difference between them is
diagnostically useful.

## Histograms and interpretation

Duration histograms retain count, sum, minimum, maximum, and cumulative fixed buckets from 100 microseconds through 10
seconds, plus an overflow bucket. The UI derives averages from sum and count. Its p95 value is the upper bound of the
first bucket containing at least 95 percent of observations, so it is an approximation rather than an exact percentile.

All duration values use seconds at the instrumentation and transport boundaries. The UI may format them as microseconds,
milliseconds, or seconds for readability. New metrics should use stable names, explicit units, and low-cardinality
labels.

## Metrics UI

The `/metrics` route is an operator-facing page loaded through a focused typed frontend service and server function. It
displays cumulative repository timings, SQL timings, supporting counters and gauges, and the snapshot timestamp. It
refreshes every two seconds and also offers manual refresh. The page reads the in-process recorder directly through the
backend snapshot API; it does not query SQLite for captured metrics.

The page is diagnostic rather than a durable monitoring system. Restarting Dispatch clears it, multiple Dispatch
processes have independent measurements, and the fixed in-process aggregation does not provide historical comparisons or
cross-process alerting. A future external exporter should reuse the `metrics` facade rather than introduce a second
instrumentation path.

## Board loading

Board loading keeps unrelated filesystem work off the critical path. Page shells load database-backed project summaries
without inspecting every project's Git working tree; the independently loaded workspace bar owns Git status inspection.
The board resolves the selected project once and runs independent reads concurrently.

The item-section query is a dedicated indexed projection that selects only card fields, caps the description excerpt,
and computes comment counts through the comments covering index. Full descriptions, origins, versions, and other
detail-only fields stay on the item-detail path. Labels, claim sources, work groups, and run previews are loaded in
bounded batch queries rather than per item. The page cache stores the board shell and item section separately, and the
server-rendered item section seeds the client cache so hydration neither duplicates the payload in memory nor
immediately repeats its request. Repository and SQL metrics are the source of evidence when this path is changed again.
