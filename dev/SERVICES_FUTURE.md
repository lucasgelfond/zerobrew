# Homebrew Services Architecture Documentation

> **Status**: Future Feature (Out of Scope for v0)
> **Created**: January 2026
> **Purpose**: Reference documentation for future services implementation

This document captures the architecture of Homebrew's services system for future implementation in Zerobrew. Services are explicitly out of scope for v0 (see SPEC.md), but this documentation preserves the research and design work for when this feature is prioritized.

## Overview

Homebrew Services is a subcommand that manages background daemons (services) for installed formulae. It wraps macOS `launchd` and Linux `systemd` to provide a unified interface.

## Key Concepts

| Concept | Description |
|---------|-------------|
| **Service** | A background daemon defined by a formula |
| **plist** | macOS launchd property list file (XML) |
| **unit** | Linux systemd service file |
| **LaunchAgent** | User-level service (runs at login) |
| **LaunchDaemon** | System-level service (runs at boot, requires root) |

## Directory Locations

### macOS (launchd)
```
~/Library/LaunchAgents/              # User services
/Library/LaunchDaemons/              # System services (root)

# Plist naming convention:
homebrew.mxcl.<formula>.plist
```

### Linux (systemd)
```
~/.config/systemd/user/              # User services
/usr/lib/systemd/system/             # System services (root)

# Unit naming convention:
homebrew.<formula>.service
```

## Core Homebrew Classes

### `Services::System` (system.rb)

Provides platform detection and path resolution:

```ruby
module Homebrew::Services::System
  def self.launchctl?    # Is this macOS with launchctl?
  def self.systemctl?    # Is this Linux with systemd?
  def self.root?         # Running as root?
  def self.user          # Current username
  def self.boot_path     # /Library/LaunchDaemons or /usr/lib/systemd/system
  def self.user_path     # ~/Library/LaunchAgents or ~/.config/systemd/user
  def self.domain_target # "system", "user/{uid}", or "gui/{uid}"
end
```

### `Services::FormulaWrapper` (formula_wrapper.rb)

Wraps a formula to provide service-related functionality:

```ruby
class FormulaWrapper
  attr_reader :formula
  
  def service_name      # e.g., "homebrew.mxcl.postgresql@15"
  def service_file      # Path to plist/unit in Cellar
  def dest              # Path where plist/unit is installed
  def installed?        # Is formula installed?
  def plist?            # Does formula have service definition?
  def loaded?           # Is service currently loaded?
  def pid               # Current PID if running
  def exit_code         # Last exit code
  def error?            # Did service fail?
  def owner             # User running the service
  def service_startup?  # Should start at boot/login?
  
  def to_hash           # Full status as hash (for JSON output)
end
```

### `Services::Formulae` (formulae.rb)

Collection methods for finding services:

```ruby
module Formulae
  def self.available_services(loaded: nil, skip_root: false)
    # Returns array of FormulaWrapper for all installed formulae with services
  end
  
  def self.services_list
    # Returns array of hashes with service status info
  end
end
```

## Service Lifecycle

### Starting a Service

1. Check formula is installed and has service definition
2. Generate or copy plist/unit file to destination
3. Register with launchd/systemd
4. Start the service

**macOS:**
```bash
launchctl bootstrap gui/501 ~/Library/LaunchAgents/homebrew.mxcl.postgresql@15.plist
launchctl kickstart -k gui/501/homebrew.mxcl.postgresql@15
```

**Linux:**
```bash
systemctl --user daemon-reload
systemctl --user start homebrew.postgresql@15.service
```

### Stopping a Service

**macOS:**
```bash
launchctl bootout gui/501/homebrew.mxcl.postgresql@15
```

**Linux:**
```bash
systemctl --user stop homebrew.postgresql@15.service
```

### Checking Status

**macOS:**
```bash
launchctl print gui/501/homebrew.mxcl.postgresql@15
# Returns: pid, exit code, loaded file path
```

**Linux:**
```bash
systemctl --user status homebrew.postgresql@15.service
```

## Service Definition in Formulas

Formulas define services in their Ruby DSL:

```ruby
class Postgresql < Formula
  # ...
  
  service do
    run [opt_bin/"postgres", "-D", var/"postgres"]
    keep_alive true
    working_dir var
    log_path var/"log/postgresql.log"
    error_log_path var/"log/postgresql.log"
  end
end
```

Key service options:
- `run` - Command to execute
- `keep_alive` - Restart if crashes
- `working_dir` - Working directory
- `log_path` / `error_log_path` - Log file locations
- `environment_variables` - Env vars to set
- `interval` - For periodic tasks
- `cron` - Cron-style scheduling

## Proposed Zerobrew Data Model

```rust
// Service definition from formula API
pub struct ServiceDefinition {
    pub command: Vec<String>,
    pub keep_alive: bool,
    pub working_dir: Option<PathBuf>,
    pub log_path: Option<PathBuf>,
    pub error_log_path: Option<PathBuf>,
    pub environment: HashMap<String, String>,
    pub interval: Option<u64>,      // Seconds
    pub cron: Option<String>,       // Cron expression
    pub requires_root: bool,
}

// Runtime service state
pub struct ServiceState {
    pub name: String,
    pub status: ServiceStatus,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub loaded_file: Option<PathBuf>,
    pub owner: String,
}

pub enum ServiceStatus {
    Running,
    Stopped,
    Failed,
    Scheduled,  // Periodic/cron service waiting
    Unknown,
}

// Service manager interface (platform-specific impl)
pub trait ServiceManager {
    fn start(&self, formula: &str, as_root: bool) -> Result<(), Error>;
    fn stop(&self, formula: &str) -> Result<(), Error>;
    fn restart(&self, formula: &str) -> Result<(), Error>;
    fn status(&self, formula: &str) -> Result<ServiceState, Error>;
    fn enable(&self, formula: &str) -> Result<(), Error>;   // Start at login/boot
    fn disable(&self, formula: &str) -> Result<(), Error>;
    fn list(&self) -> Result<Vec<ServiceState>, Error>;
}

// Platform implementations
pub struct LaunchdManager { /* macOS */ }
pub struct SystemdManager { /* Linux */ }
```

## Plist Template (macOS)

Generated from formula service definition:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>homebrew.mxcl.postgresql@15</string>
  <key>ProgramArguments</key>
  <array>
    <string>/opt/homebrew/opt/postgresql@15/bin/postgres</string>
    <string>-D</string>
    <string>/opt/homebrew/var/postgresql@15</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
  <key>WorkingDirectory</key>
  <string>/opt/homebrew/var</string>
  <key>StandardOutPath</key>
  <string>/opt/homebrew/var/log/postgresql@15.log</string>
  <key>StandardErrorPath</key>
  <string>/opt/homebrew/var/log/postgresql@15.log</string>
</dict>
</plist>
```

## Systemd Unit Template (Linux)

```ini
[Unit]
Description=Homebrew: postgresql@15
After=network.target

[Service]
Type=simple
ExecStart=/home/linuxbrew/.linuxbrew/opt/postgresql@15/bin/postgres -D /home/linuxbrew/.linuxbrew/var/postgresql@15
WorkingDirectory=/home/linuxbrew/.linuxbrew/var
Restart=always
StandardOutput=append:/home/linuxbrew/.linuxbrew/var/log/postgresql@15.log
StandardError=append:/home/linuxbrew/.linuxbrew/var/log/postgresql@15.log

[Install]
WantedBy=default.target
```

## API Data Source

Service definitions come from the Homebrew formula API. The `service` block in formulas is serialized to JSON:

```json
{
  "name": "postgresql@15",
  "service": {
    "run": ["/opt/homebrew/opt/postgresql@15/bin/postgres", "-D", "/opt/homebrew/var/postgresql@15"],
    "keep_alive": true,
    "working_dir": "/opt/homebrew/var",
    "log_path": "/opt/homebrew/var/log/postgresql@15.log",
    "error_log_path": "/opt/homebrew/var/log/postgresql@15.log"
  }
}
```

## Implementation Complexity Estimate

| Component | Effort | Notes |
|-----------|--------|-------|
| Service definition parsing | Low | JSON from API |
| Plist generation | Medium | XML templating |
| Systemd unit generation | Medium | INI templating |
| launchctl integration | High | Complex domain/bootstrap API |
| systemctl integration | Medium | Simpler CLI interface |
| Status monitoring | Medium | Parse command output |
| Testing | High | Requires root, platform-specific |

**Estimated total: 2-4 weeks of focused development**

## Proposed CLI Commands

```bash
zb services list                    # List all available services
zb services start <formula>         # Start a service
zb services stop <formula>          # Stop a service  
zb services restart <formula>       # Restart a service
zb services status <formula>        # Show service status
zb services enable <formula>        # Enable auto-start
zb services disable <formula>       # Disable auto-start
```

## Migration Considerations

When migrating from Homebrew, services present a special challenge:
- Running services should be stopped before uninstalling from Homebrew
- Service state (enabled/disabled) could be preserved
- Plist/unit files may need to be regenerated with Zerobrew paths

Current migration implementation warns users about running services but does not attempt to migrate them.

## References

- Homebrew source: `brew/Library/Homebrew/services/`
- Key files analyzed:
  - `system.rb` - Platform detection and paths
  - `formula_wrapper.rb` - Service status and management
  - `formulae.rb` - Service discovery
  - `commands/start.rb` - Start implementation