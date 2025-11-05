# SV2D Development Roadmap

## Project Vision
Build a plug-and-play Stratum V2 infrastructure tool for sovereign miners - as easy as Umbrel's SV1 pools, but with SV2 benefits.

**Target Users:** ckpool miners who want to run their own SV2 infrastructure without complexity

**Key Value:** Connect any miner (legacy SV1 or new SV2) to your own pool with one command

---

## Phase 1: Stability ✅ COMPLETED

**Goal:** Fix critical issues from workshop failure - make current system stable and usable

### Completed Tasks

- [x] **Task 1: Process Monitoring** - Auto-restart crashed components
  - Health checking every 10 seconds
  - Exponential backoff on restart (1s, 2s, 4s, 8s...)
  - Give up after 10 consecutive failures
  - Track restart count in status
  - Completed: commit 3423438

- [x] **Task 2: Fix Stop Command** - Daemon should exit when stopped
  - `sv2-cli stop` kills sv2d daemon completely
  - No orphaned processes
  - Clean restart after stop
  - Completed: commit 1c9130f

- [x] **Task 3: Improve Error Messages** - Make errors actionable
  - Created comprehensive documentation with troubleshooting
  - Bitcoin Core setup guide with common issues
  - Workshop-style config examples with inline help
  - Completed: commit 5a1cb31

- [x] **Task 4: Smart Path Resolution** - Find binaries intelligently
  - Auto-detect Bitcoin Core in common locations
  - Search for sv2-tp in multiple directories
  - Clear errors showing where we looked
  - Completed: commit b29340a

### Additional Achievements

- [x] **Fixed Build System** - Resolved 80+ compilation errors
  - Created missing sv2-core/src/types.rs (365 lines)
  - Created missing sv2-core/src/protocol.rs (120 lines)
  - Updated module declarations
  - Build status: Clean with zero warnings

- [x] **Workshop-Style Documentation** - Following sv2-workshop best practices
  - Created examples/configs/README.md (279 lines)
  - Created examples/configs/bitaxe-regtest.toml (215 lines)
  - Created examples/configs/bitaxe-signet.toml (243 lines)
  - Created BITCOIN_SETUP.md with setup guides

- [x] **Project Cleanup** - Prepared for sv2-node transition
  - Removed backup and WORKING config files
  - Added comprehensive build documentation
  - Clean git history and structure

### Success Criteria - ACHIEVED

✅ System runs stable for 4+ hours without manual intervention
✅ Components auto-restart when crashed
✅ Stop command fully shuts down daemon
✅ Documentation tells users exactly what to do
✅ Clean codebase builds successfully
✅ Ready for Phase 2 transition

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

## Phase 2: Single Binary Refactor (NEXT - Starting Now)

**Goal:** Simplify architecture to true plug-and-play experience

**Status:** Ready to begin - Phase 1 complete, codebase clean and stable

### Next Steps

1. **Create sv2-node binary structure**
   - New crate: `sv2-node` (merges sv2d + sv2-cli functionality)
   - Single entry point with subcommands
   - Preserve sv2-core as library

2. **Implement unified command interface**
   ```bash
   sv2-node start [--pool|--solo|--proxy]
   sv2-node stop
   sv2-node status
   sv2-node logs [--follow]
   sv2-node config [--generate|--validate]
   ```

3. **Bundle SRI components** (optional, evaluate feasibility)
   - Consider embedding pool_sv2 + translator_sv2
   - OR continue using external sv2-tp (simpler, already working)
   - Decision point: Simplicity vs maintenance

4. **Auto-configuration system**
   - Detect Bitcoin Core installation
   - Auto-generate config on first run
   - Interactive setup wizard (optional)
   - Validate RPC connection before starting

5. **Simplified modes**
   - `sv2-node start --solo` - Solo mining (default)
   - `sv2-node start --pool` - Run your own pool
   - `sv2-node start --proxy` - Connect to upstream (future)

### Architecture Goals

- Zero config for first run (sensible defaults)
- Automatic Bitcoin node detection
- Built-in SV1→SV2 translation (via sv2-tp)
- Cross-platform binaries (macOS, Linux)
- Clear status feedback with actionable errors
- Single binary distribution
- Backward compatible with existing configs

### Pre-work Completed ✅

- [x] Build system fixed and stable
- [x] Core types and protocol modules in place
- [x] Workshop-style documentation and examples
- [x] Clean project structure
- [x] Comprehensive setup guides

### Implementation Plan

**Week 1: Core Refactor**
- Create sv2-node crate structure
- Merge sv2d + sv2-cli command logic
- Implement unified CLI interface
- Test: All existing functionality works via sv2-node

**Week 2: Auto-configuration**
- Bitcoin Core auto-detection
- Config generation on first run
- Interactive setup for edge cases
- Test: Fresh install works without manual config

**Week 3: Polish & Testing**
- Cross-platform testing (macOS, Linux)
- Error message improvements
- Documentation updates
- Test: 2 users can install and mine in < 5 minutes

**Week 4: Release Preparation**
- GitHub releases with binaries
- Installation script
- Migration guide from sv2d to sv2-node
- Public beta announcement

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

- **Week 1 (Nov 5):** ✅ Phase 1 complete - Stability fixes, build fixes, documentation
- **Week 2-3 (Nov 12-19):** Phase 2 - sv2-node refactor and auto-configuration
- **Week 4 (Nov 26):** Testing, polish, and release preparation
- **Week 5 (Dec 3):** Public beta release

---

## Success Metrics

**Phase 1 (Completed):**
- ✅ Build system working with zero warnings
- ✅ Process monitoring and auto-restart functional
- ✅ Stop command cleanly shuts down all components
- ✅ Comprehensive documentation and examples created
- ✅ Clean codebase ready for sv2-node transition

**Phase 2 (Next - sv2-node):**
- Single binary `sv2-node` replaces `sv2d` + `sv2-cli`
- Fresh install to mining in < 5 minutes
- Works on macOS and Linux out of box
- Zero-config first run with sensible defaults
- Backward compatible with existing sv2d configs

**Phase 3 (Future):**
- 10+ users running in production
- < 5% support requests (means it's intuitive)
- No critical bugs reported
- Community contributions and feedback

---

## Current Status

**Last Updated:** 2025-11-05
**Current Phase:** Phase 2 (sv2-node Refactor)
**Status:** Ready to Begin

**Recent Achievements:**
- Fixed 80+ compilation errors (created types.rs, protocol.rs)
- Added workshop-style documentation (737 lines)
- Created comprehensive setup guides
- Cleaned project structure for sv2-node transition
- All Phase 1 stability tasks completed

**Next Immediate Actions:**
1. Create sv2-node crate structure
2. Merge sv2d + sv2-cli command interfaces
3. Implement auto-configuration system
4. Test unified binary functionality

**GitHub:** https://github.com/xyephy/stratum-v2-tools
**Latest Commit:** 5a1cb31 - Fix build errors and add workshop-style documentation
