# SV2D Development Roadmap

## Project Vision
Build a plug-and-play Stratum V2 infrastructure tool for sovereign miners - as easy as Umbrel's SV1 pools, but with SV2 benefits.

**Target Users:** ckpool miners who want to run their own SV2 infrastructure without complexity

**Key Value:** Connect any miner (legacy SV1 or new SV2) to your own pool with one command

---

## Phase 1: Stability (This Week)

**Goal:** Fix critical issues from workshop failure - make current system stable and usable

### Tasks

- [ ] **Task 1: Process Monitoring** - Auto-restart crashed components
  - Health checking every 10 seconds
  - Exponential backoff on restart (1s, 2s, 4s, 8s...)
  - Give up after 10 consecutive failures
  - Track restart count in status
  - Test: System runs 4+ hours without intervention

- [ ] **Task 2: Fix Stop Command** - Daemon should exit when stopped
  - `sv2-cli stop` should kill sv2d daemon completely
  - No orphaned processes
  - Clean restart after stop
  - Test: `ps aux | grep sv2d` shows nothing after stop

- [ ] **Task 3: Improve Error Messages** - Make errors actionable
  - Every error explains WHAT, WHY, and HOW TO FIX
  - Use emoji and formatting for readability
  - Include actual values (ports, paths, etc.)
  - Test: Non-technical users understand what to do

- [ ] **Task 4: Smart Path Resolution** - Find binaries intelligently
  - Auto-detect Bitcoin Core in common locations
  - Search for sv2-tp in multiple directories
  - Offer to download missing components
  - Clear errors showing where we looked
  - Test: Works on 3 different machines

### Success Criteria

✅ System runs stable for 4+ hours without manual intervention
✅ Components auto-restart when crashed
✅ Stop command fully shuts down daemon
✅ Errors tell users exactly what to do
✅ Fresh machine installation gives actionable feedback
✅ 2 test users can install and start mining without asking for help

### Testing Requirements

**Automated:**
- Start/stop cycle 10 times
- Kill components randomly - verify auto-restart
- Run for 4 hours continuously

**User Testing:**
- Give 2 people only the install URL
- Watch them install (don't help)
- Document confusion points
- Fix top 3 issues
- Repeat until successful

---

## Phase 2: Single Binary Refactor (Next Week)

**Goal:** Simplify architecture to true plug-and-play experience

### Changes

- **Merge binaries:** `sv2d` + `sv2-cli` → single `sv2-node` binary
- **Bundle components:** Include SRI pool + translator in binary
- **Auto-configuration:** Detect Bitcoin Core or accept RPC URL
- **Simple modes:**
  - `sv2-node start --pool` - Run your own pool
  - `sv2-node start --solo` - Solo mining
  - `sv2-node start --proxy` - Connect to upstream (future)
- **One-command install:**
  ```bash
  curl -L https://github.com/YOU/sv2-node/releases/latest/download/install.sh | bash
  sv2-node start
  # Point miner to localhost:3333
  ```

### Architecture Goals

- Zero config for first run
- Automatic Bitcoin node detection
- Built-in SV1→SV2 translation (legacy miner support)
- Cross-platform binaries (macOS, Linux)
- Clear status feedback

---

## Phase 3: UX Polish (Future)

### Features

- **Web Dashboard:** Real-time monitoring via browser
- **Metrics:** Hashrate, shares, earnings tracking
- **Multi-platform:** Windows support
- **Hardware Detection:** Auto-detect connected miners
- **Pool Templates:** Pre-configured upstream pool connections
- **Update System:** Auto-update to latest versions

### Integration Ideas

- Umbrel app package
- Start9 service
- RaspiBlitz integration
- Docker images

---

## What We Learned (Workshop Failure)

### Problems Encountered

1. **Hardcoded paths** - Binaries not found on user machines
2. **Silent failures** - Components crashed, system said "running"
3. **No auto-restart** - Components died after ~10 minutes
4. **Unhelpful errors** - Users didn't know what to fix
5. **Stop didn't work** - Daemon stayed running after stop
6. **Too complex** - Download 3 things, configure 3 files, run 4 commands

### What We're Fixing

1. **Smart discovery** - Find binaries wherever they are
2. **Health monitoring** - Detect crashes and auto-restart
3. **Process monitoring** - Always know component status
4. **Actionable errors** - Tell users exactly what to do
5. **Clean shutdown** - Stop means everything stops
6. **Simplification** - One binary, one command, works

### Design Principles Going Forward

- **Test on fresh machines** - Before any release
- **Real user testing** - Watch non-technical users try it
- **Fail loudly** - Never pretend something works when it doesn't
- **Guide, don't assume** - Error messages are documentation
- **Stability > features** - Working simply > broken complexity

---

## Timeline

- **Week 1 (Current):** Stability fixes (Phase 1)
- **Week 2:** Single binary refactor (Phase 2)
- **Week 3:** Testing and iteration
- **Week 4:** Public beta release

---

## Success Metrics

**This Week:**
- 4-hour stability test passes
- 2 users install successfully without help

**Next Week:**
- Fresh install to mining in < 5 minutes
- Works on macOS and Linux out of box

**One Month:**
- 10+ users running in production
- < 5% support requests (means it's intuitive)
- No critical bugs reported

---

**Last Updated:** 2025-10-26
**Current Phase:** Phase 1 (Stability)
**Status:** In Progress
