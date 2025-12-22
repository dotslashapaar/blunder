# Blunder - Solana's Adaptive Scheduler

#### Blunder is a custom Solana scheduler that does it all — Jito-style bundles, Rakurai-level dense packing, automatic scheduler switching (no manual Agave config needed), and even has external schedulers as plug-ins. Basically, it supports everything.

## Blunder - Quick Start Guide

### 1. Clone the repository and build

```bash
git clone https://github.com/dotslashapaar/blunder.git
cd blunder
cargo build --release
```


---

### 2. Run Engine with Internal Scheduler (TPU)

```bash
cargo run --bin blunder-tpu --release
```


---

### 3. To try External Scheduler setup:

**Terminal 1: Run External Scheduler Server**

```bash
cargo run --bin blunder-scheduler-external --release
```

**Terminal 2: Run Engine with External Scheduler**

```bash
cargo run --bin blunder-tpu --release -- --external-scheduler 127.0.0.1:8080
```


---

### Notes

- No need to `cd` into subfolders to run binaries, use `--bin <binary_name>`.
- The `--external-scheduler` flag enables TPU engine to connect to external scheduler over QUIC.
- Replace `127.0.0.1:8080` with actual external scheduler address as needed.
- Use `--release` for optimized builds.

---
<br>

## Complete Transaction Lifecycle Architecture
 
```
Block Engine ─> TPU Pipeline ─> Scheduler ─> Workers ─> Executors

┌───────────────────────────────────────────────────────┐
│ BLOCK ENGINE (Off-chain, parallel)                    │
│ • Runs 0-50ms concurrently with network               │   <─ From here
│ • Collects searcher bundles and loose transactions    │
│ • Simulates bundles                                   │
│ • First-price sealed-bid auction                      │
│ • Conflict detection via greedy knapsack              │
│ • Outputs winning bundles                             │
│ Thread: 1 (separate from TPU)                         │
│ Status: Verified                                      │
└───────────────┬───────────────────────────────────────┘
                │
        ┌───────┴────────┐
        ↓                ↓
 Winning Bundles      Loose Transactions
        │                │
        └───────┬────────┘
                ↓
   ┌─────────────────────────────────────────┐
   │ TPU PIPELINE                            │
   │                                         │
   │ STAGE 1: INGESTION (5ms)                │
   │ • Receive bundles and transactions      │
   │ • Queue for processing                  │
   │ Thread: 1                               │
   │ Status: Standard Solana                 │
   └─────────────┬───────────────────────────┘
                 ↓
   ┌─────────────────────────────────────────┐
   │ STAGE 2: SIGVERIFY (10ms)               │
   │ • Verify signatures                     │
   │ • Deduplicate transactions              │
   │ • Bundles pre-verified                  │
   │ Threads: 4-8 parallel                   │
   │ Status: Standard Solana                 │
   └─────────────┬───────────────────────────┘
                 ↓
   ┌─────────────────────────────────────────────┐
   │ STAGE 3: PRIORITIZER (5ms)                  │
   │ Fee-per-CU efficiency ordering              │
   │                                             │
   │ Algorithm:                                  │
   │   Score = (fee × 1,000,000) / CU            │
   │   • Maximizes validator revenue per CU      │
   │   • Rewards efficient transactions          │
   │                                             │
   │ Process:                                    │
   │   1. Calculate fee-per-CU score             │
   │   2. Sort descending                        │
   │   3. Output efficiency-ranked queue         │
   │                                             │
   │ Result:                                     │
   │   • Dense blocks (max fees in 48M CU)       │
   │   • Efficient txs prioritized               │
   │   • Aligned with validator incentives       │
   │ Thread: 1                                   │
   │ Status: Fee-per-CU prioritization           │
   └─────────────┬───────────────────────────────┘
                 ↓
   ┌─────────────────────────────────────────┐
   │ STAGE 4: METADATA WRAPPER               │
   │ • Add bundle_id to transactions         │
   │ • Mark atomic=true (bundle txs)         │
   │ • Mark atomic=false (loose txs)         │
   │ Thread: inline                          │
   │ Status: Verified                        │
   └─────────────┬───────────────────────────┘
                 ↓
       ┌─────────┴─────────┐
       ↓                   ↓
   ┌─────────────┐      ┌─────────────┐
   │ ADAPTIVE    │      │ EXTERNAL    │
   │ SCHEDULER   │      │ SCHEDULER   │
   │ (Internal)  │      │ (IPC/QUIC)  │
   └───────┬─────┘      └───────┬─────┘
           │                    │
           └─────────┬──────────┘
                     ↓
   ┌───────────────────────────────────┐
   │ STAGE 5: SCHEDULER (5ms)          │
   │ • Route bundles to 1 worker       │
   │ • Route loose txs to any worker   │
   │ • Conflict checking               │
   │ Thread: 1                         │
   │ Status: Verified                  │
   └─────────────┬─────────────────────┘
                 ↓
   ┌───────────────────────────────────┐
   │ STAGE 6: WORKERS (240ms)          │
   │ • 6 threads (4 non-vote + 2 vote) │
   │ • Bundle: lock all → execute seq  │
   │ • Loose: lock → execute → unlock  │
   │ Status: Verified                  │
   └─────────────┬─────────────────────┘
                 ↓
   ┌─────────────────────────────────┐
   │ STAGE 7: EXECUTORS (parallel)   │
   │ • 4 parallel threads            │
   │ Status: Verified                │      <─ Till here
   └─────────────┬───────────────────┘  
                 ↓
   ┌────────────────────────────────┐
   │ STAGE 8: POH + BROADCAST       │
   │ • Standard Solana PoH          │
   │ • ~70ms duration               │
   │ Status: Unchanged              │ 
   └────────────────────────────────┘

```

---

## Crate Responsibilities in Blunder

- **core/**
    - Provides core data structures like bundles and transactions.
    - Defines the external scheduler protocol and message serialization.
    - Includes shared traits, metadata, and error handling used across the project.
    - Contains external scheduler client implementation.
- **engine/**
    - Implements the main execution engine for processing bundles and transactions.
    - Runs the integrated TPU pipeline: ingestion, signature verification, prioritization, and metadata enrichment.
    - Manages worker pools and the scheduling engine interface.
    - Contains executors and async worker implementations.
- **scheduler/**
    - Houses various scheduling algorithm implementations:
        - Adaptive scheduler: dynamic, load-aware scheduling.
        - Greedy scheduler: priority-based.
        - Priority graph scheduling for conflict resolution.
        - Load monitoring utilities.
    - Provides the scheduling logic as pluggable modules.
- **scheduler-external/**
    - External scheduler service running QUIC server with TLS.
    - Uses the external scheduler protocol to communicate with the engine.
    - Implements round-robin scheduling strategy.
    - Acts as a scalable, standalone scheduler plugin.

***
