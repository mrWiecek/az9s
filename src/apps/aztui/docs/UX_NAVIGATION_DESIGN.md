# aztui UI/UX Navigation Design

## Overview

This document defines the UI/UX and navigation design for aztui, with a focus on achieving **k9s-like usability and interaction fidelity**. The goal is to create a keyboard-first, information-dense, highly discoverable terminal interface for Azure resource management.

---

## Design Principles

### 1. Keyboard-First, Always
- **Every action** must be accessible via keyboard
- **No mouse required** for any workflow
- **Vim-style navigation** as the primary paradigm
- **Mnemonic keybindings** for discoverability (e.g., `r` for refresh, `d` for delete)

### 2. Information Density
- **Maximize visible data** without overwhelming the user
- **Multi-column layouts** for resource lists
- **Compact but readable** typography
- **Smart truncation** with full-text tooltips/overlays

### 3. Focus State Clarity
- **Always visible** focus indicator
- **Distinct visual hierarchy** between focused and unfocused panes
- **Breadcrumb trail** showing current location in resource hierarchy
- **Status bar** with context-aware information

### 4. Command Discoverability
- **Context-aware help** (press `?` anywhere)
- **Footer keybinding hints** that change per view
- **Command palette** (Ctrl+P) with fuzzy search
- **Progressive disclosure** - show common commands first

### 5. Intentional Interaction Design
- **Confirmation for destructive actions** (delete, stop, etc.)
- **Immediate feedback** for all actions (loading states, success/error)
- **Undo/redo** where possible
- **Consistent patterns** across all views

---

## Screen Hierarchy

### Level 1: Subscription List (Entry Point)
```
┌─────────────────────────────────────────────────────────────────────────┐
│ aztui > Subscriptions                                            [⟳]    │
├─────────────────────────────────────────────────────────────────────────┤
│ NAME                                    STATE      SUBSCRIPTION ID      │
│ ▸ Production Subscription               Enabled    12345678-1234-...   │
│ ▸ Development Subscription              Enabled    87654321-4321-...   │
│ ▸ Staging Subscription                  Enabled    abcdef12-5678-...   │
│                                                                         │
│ [3 subscriptions]                                                       │
├─────────────────────────────────────────────────────────────────────────┤
│ ↑↓/jk: Navigate  Enter: Select  r: Refresh  /: Filter  ?: Help  q: Quit│
└─────────────────────────────────────────────────────────────────────────┘
```

**Key Features**:
- **Breadcrumb**: `aztui > Subscriptions`
- **Loading indicator**: `[⟳]` when fetching
- **Column headers**: Clear data organization
- **Selection indicator**: `▸` for current item
- **Item count**: `[3 subscriptions]` for context
- **Context-aware footer**: Shows relevant commands

### Level 2: Resource Group List
```
┌─────────────────────────────────────────────────────────────────────────┐
│ aztui > Production Subscription > Resource Groups                      │
├─────────────────────────────────────────────────────────────────────────┤
│ NAME                    LOCATION        RESOURCES    TAGS               │
│ ▸ rg-production-web     eastus          12           env:prod, app:web │
│   rg-production-db      eastus          5            env:prod, app:db  │
│   rg-production-cache   westus2         3            env:prod          │
│   rg-staging-web        eastus          8            env:staging       │
│                                                                         │
│ [4 resource groups]                                                     │
├─────────────────────────────────────────────────────────────────────────┤
│ Esc: Back  ↑↓/jk: Navigate  Enter: View Resources  r: Refresh  ?: Help │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key Features**:
- **Breadcrumb trail**: Shows navigation path
- **Multi-column layout**: Name, Location, Resource count, Tags
- **Tag display**: Inline, comma-separated
- **Resource count**: Quick overview of group size
- **Back navigation**: `Esc` to return to subscriptions

### Level 3: Resource List
```
┌─────────────────────────────────────────────────────────────────────────┐
│ aztui > Production > rg-production-web > Resources          Filter: vm │
├─────────────────────────────────────────────────────────────────────────┤
│ TYPE                NAME                    LOCATION    STATUS          │
│ ▸ VirtualMachine    vm-web-01               eastus      ● Running      │
│   VirtualMachine    vm-web-02               eastus      ● Running      │
│   LoadBalancer      lb-web                  eastus      ● Succeeded    │
│   PublicIPAddress   pip-web-lb              eastus      ● Succeeded    │
│   NetworkInterface  nic-web-01              eastus      ● Succeeded    │
│   NetworkInterface  nic-web-02              eastus      ● Succeeded    │
│   StorageAccount    stprodweb01             eastus      ● Available    │
│   KeyVault          kv-prod-web             eastus      ● Available    │
│                                                                         │
│ [8 resources, 2 filtered]                                               │
├─────────────────────────────────────────────────────────────────────────┤
│ Esc: Back  Enter: Details  s: Start  t: Stop  d: Delete  /: Filter     │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key Features**:
- **Active filter indicator**: `Filter: vm` in breadcrumb
- **Type-first layout**: Resource type as first column
- **Status indicators**: `●` with color coding (green=running, yellow=transitioning, red=stopped)
- **Filtered count**: `[8 resources, 2 filtered]`
- **Action hints**: Context-specific commands (start, stop, delete)

### Level 4: Resource Detail (Split View)
```
┌─────────────────────────────────────────────────────────────────────────┐
│ aztui > Production > rg-production-web > vm-web-01                      │
├──────────────────────────────────┬──────────────────────────────────────┤
│ PROPERTIES                       │ METRICS (Last 1h)                    │
│                                  │                                      │
│ Name: vm-web-01                  │ CPU Usage:        ▂▃▅▇█▇▅▃▂  45%    │
│ Type: VirtualMachine             │ Memory:           ▃▃▄▄▅▅▄▃▃  62%    │
│ Location: eastus                 │ Network In:       ▁▂▂▃▃▂▂▁▁  2.3MB  │
│ Status: ● Running                │ Network Out:      ▁▁▂▂▂▂▁▁▁  1.1MB  │
│ Size: Standard_D2s_v3            │ Disk Read:        ▁▁▁▂▁▁▁▁▁  120KB  │
│ OS: Ubuntu 22.04 LTS             │ Disk Write:       ▁▂▃▂▁▁▁▁▁  340KB  │
│                                  │                                      │
│ Resource ID:                     │ RECENT ACTIVITY                      │
│ /subscriptions/12345678.../...   │                                      │
│                                  │ 2024-01-15 14:32  VM started         │
│ Tags:                            │ 2024-01-15 10:15  Config updated     │
│   env: production                │ 2024-01-14 09:00  VM restarted       │
│   app: web                       │                                      │
│   owner: platform-team           │                                      │
│                                  │                                      │
├──────────────────────────────────┴──────────────────────────────────────┤
│ Esc: Back  s: Start  t: Stop  r: Restart  l: Logs  m: Metrics  ?: Help │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key Features**:
- **Split-pane layout**: Properties on left, metrics/activity on right
- **Inline sparklines**: Visual metrics without leaving the view
- **Full resource ID**: Copyable (future: Ctrl+C to copy)
- **Tag list**: Vertical layout for readability
- **Recent activity**: Quick access to audit log
- **Resource-specific actions**: Start, stop, restart for VMs

---

## Pane Composition & Focus Management

### Multi-Pane Layout System

#### Horizontal Split
```
┌─────────────────────────────────────────────────────────────────────────┐
│ aztui > Multi-Pane View                                                 │
├─────────────────────────────────────────────────────────────────────────┤
│ ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓ ┌──────────────────────────────────┐ │
│ ┃ SUBSCRIPTIONS (FOCUSED)        ┃ │ RESOURCE GROUPS                  │ │
│ ┃                                ┃ │                                  │ │
│ ┃ ▸ Production Subscription      ┃ │   rg-production-web              │ │
│ ┃   Development Subscription     ┃ │   rg-production-db               │ │
│ ┃   Staging Subscription         ┃ │   rg-production-cache            │ │
│ ┃                                ┃ │                                  │ │
│ ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛ └──────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────────┤
│ Tab: Switch Pane  Ctrl+W: Close Pane  Ctrl+H/L: Resize  ?: Help        │
└─────────────────────────────────────────────────────────────────────────┘
```

**Focus Indicators**:
- **Focused pane**: Double-line border (`┏━━┓`)
- **Unfocused pane**: Single-line border (`┌──┐`)
- **Focused pane title**: Bold or highlighted
- **Cursor/selection**: Only visible in focused pane

#### Vertical Split
```
┌─────────────────────────────────────────────────────────────────────────┐
│ aztui > Resource Detail                                                 │
├─────────────────────────────────────────────────────────────────────────┤
│ ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓ │
│ ┃ PROPERTIES (FOCUSED)                                                 ┃ │
│ ┃ Name: vm-web-01                                                      ┃ │
│ ┃ Type: VirtualMachine                                                 ┃ │
│ ┃ Status: ● Running                                                    ┃ │
│ ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛ │
│ ┌──────────────────────────────────────────────────────────────────────┐ │
│ │ LOGS                                                                 │ │
│ │ 2024-01-15 14:32:15 [INFO] Application started                      │ │
│ │ 2024-01-15 14:32:16 [INFO] Listening on port 8080                   │ │
│ └──────────────────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────────────┤
│ Tab: Switch Pane  Ctrl+W: Close Pane  Ctrl+J/K: Resize  ?: Help        │
└─────────────────────────────────────────────────────────────────────────┘
```

### Pane Management Keybindings

| Key | Action | Description |
|-----|--------|-------------|
| `Tab` | Switch pane | Cycle through panes in order |
| `Shift+Tab` | Switch pane (reverse) | Cycle backwards |
| `Ctrl+W` | Close pane | Close current pane |
| `Ctrl+H` | Resize left | Decrease pane width |
| `Ctrl+L` | Resize right | Increase pane width |
| `Ctrl+J` | Resize down | Decrease pane height |
| `Ctrl+K` | Resize up | Increase pane height |
| `Ctrl+\` | Split horizontal | Create horizontal split |
| `Ctrl+/` | Split vertical | Create vertical split |

---

## Status Bar Design

### Comprehensive Status Bar
```
┌─────────────────────────────────────────────────────────────────────────┐
│ ● Connected | Sub: Production | RG: rg-web | 12 resources | ⟳ 2m ago   │
└─────────────────────────────────────────────────────────────────────────┘
```

**Components** (left to right):
1. **Connection status**: `● Connected` (green), `● Disconnected` (red), `● Connecting` (yellow)
2. **Current subscription**: Abbreviated name
3. **Current resource group**: If applicable
4. **Resource count**: In current view
5. **Last refresh**: Time since last data fetch

### Context-Aware Status Bar

**Subscription List View**:
```
│ ● Connected | 3 subscriptions | Last refresh: 2m ago                    │
```

**Resource List View**:
```
│ ● Connected | Production | rg-web | 12 resources | Filter: vm | 2m ago │
```

**Resource Detail View**:
```
│ ● Connected | vm-web-01 | Status: Running | CPU: 45% | Mem: 62%        │
```

---

## Breadcrumb Navigation

### Breadcrumb Design
```
aztui > Production Subscription > rg-production-web > Resources
```

**Features**:
- **Clickable segments** (future: mouse support)
- **Keyboard navigation**: `Ctrl+B` to focus breadcrumb, arrow keys to navigate
- **Truncation**: Smart truncation for long names
  - `aztui > Production... > rg-prod... > Resources`
- **Separator**: ` > ` for clarity

### Breadcrumb Interactions

| Key | Action |
|-----|--------|
| `Ctrl+B` | Focus breadcrumb |
| `←/→` | Navigate breadcrumb segments |
| `Enter` | Jump to selected segment |
| `Esc` | Return to main view |

---

## Command Palette

### Design
```
┌─────────────────────────────────────────────────────────────────────────┐
│                          Command Palette                                │
├─────────────────────────────────────────────────────────────────────────┤
│ > refresh_                                                              │
├─────────────────────────────────────────────────────────────────────────┤
│ ▸ Refresh Current View                                          Ctrl+R │
│   Refresh All Subscriptions                                            │
│   Refresh Resource Groups                                              │
│   Refresh Resources                                                     │
│                                                                         │
│ [4 commands]                                                            │
├─────────────────────────────────────────────────────────────────────────┤
│ Type to search, ↑↓ to navigate, Enter to execute, Esc to cancel        │
└─────────────────────────────────────────────────────────────────────────┘
```

**Features**:
- **Fuzzy search**: Type `ref` to find `Refresh`
- **Keyboard shortcuts**: Displayed on the right
- **Command categories**: Group related commands
- **Recent commands**: Show recently used commands first
- **Command preview**: Show command description on hover

### Command Categories

1. **Navigation**
   - Go to Subscriptions
   - Go to Resource Groups
   - Go to Resources
   - Go to Logs
   - Back

2. **Actions**
   - Refresh Current View
   - Refresh All
   - Start Resource
   - Stop Resource
   - Restart Resource
   - Delete Resource

3. **View**
   - Toggle Split View
   - Toggle Metrics
   - Toggle Logs
   - Toggle JSON View

4. **Filter**
   - Filter by Type
   - Filter by Location
   - Filter by Tag
   - Clear Filter

5. **Help**
   - Show Help
   - Show Keyboard Shortcuts
   - Show About

---

## Keyboard Shortcuts Reference

### Global Shortcuts (Always Available)

| Key | Action | Description |
|-----|--------|-------------|
| `q` | Quit | Exit application |
| `Ctrl+C` | Force Quit | Immediate exit |
| `Ctrl+Q` | Quit | Alternative quit |
| `?` | Help | Show context-aware help |
| `Ctrl+P` | Command Palette | Open command palette |
| `/` | Filter | Start filter mode |
| `Esc` | Back/Cancel | Navigate back or cancel action |
| `Ctrl+R` | Refresh | Refresh current view |

### Navigation Shortcuts

| Key | Action | Description |
|-----|--------|-------------|
| `↑` / `k` | Up | Move selection up |
| `↓` / `j` | Down | Move selection down |
| `←` / `h` | Left | Move left (in multi-column) |
| `→` / `l` | Right | Move right (in multi-column) |
| `g` | First | Jump to first item |
| `G` | Last | Jump to last item |
| `Ctrl+U` | Page Up | Scroll up one page |
| `Ctrl+D` | Page Down | Scroll down one page |
| `PgUp` | Page Up | Scroll up one page |
| `PgDn` | Page Down | Scroll down one page |
| `Home` | First | Jump to first item |
| `End` | Last | Jump to last item |

### Selection & Action Shortcuts

| Key | Action | Description |
|-----|--------|-------------|
| `Enter` | Select | Select current item / drill down |
| `Space` | Toggle | Toggle selection (multi-select) |
| `Ctrl+A` | Select All | Select all items |
| `Ctrl+D` | Deselect All | Clear selection |

### Resource Action Shortcuts

| Key | Action | Description |
|-----|--------|-------------|
| `s` | Start | Start resource (VM, App Service, etc.) |
| `t` | Stop | Stop resource |
| `r` | Restart | Restart resource |
| `d` | Delete | Delete resource (with confirmation) |
| `e` | Edit | Edit resource properties |
| `c` | Copy | Copy resource ID to clipboard |
| `l` | Logs | View resource logs |
| `m` | Metrics | View resource metrics |

### View Shortcuts

| Key | Action | Description |
|-----|--------|-------------|
| `1` | Subscriptions | Go to subscription list |
| `2` | Resource Groups | Go to resource group list |
| `3` | Resources | Go to resource list |
| `4` | Logs | Go to logs view |
| `Tab` | Next Pane | Switch to next pane |
| `Shift+Tab` | Previous Pane | Switch to previous pane |
| `Ctrl+\` | Split Horizontal | Create horizontal split |
| `Ctrl+/` | Split Vertical | Create vertical split |
| `Ctrl+W` | Close Pane | Close current pane |

---

## Information Density Optimization

### List View Optimization

**Before (Low Density)**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│ Name: vm-web-01                                                         │
│ Type: VirtualMachine                                                    │
│ Location: eastus                                                        │
│ Status: Running                                                         │
│                                                                         │
│ Name: vm-web-02                                                         │
│ Type: VirtualMachine                                                    │
│ Location: eastus                                                        │
│ Status: Running                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**After (High Density)**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│ TYPE              NAME          LOCATION    STATUS      SIZE            │
│ VirtualMachine    vm-web-01     eastus      ● Running   Standard_D2s_v3│
│ VirtualMachine    vm-web-02     eastus      ● Running   Standard_D2s_v3│
│ LoadBalancer      lb-web        eastus      ● Active    Standard        │
│ StorageAccount    stprodweb01   eastus      ● Available Standard_LRS   │
└─────────────────────────────────────────────────────────────────────────┘
```

**Improvements**:
- **4x more items** visible in same space
- **Scannable columns** for quick comparison
- **Visual indicators** (●) for status
- **Aligned data** for easy reading

### Smart Truncation

**Long Names**:
```
│ very-long-resource-group-name-that-exceeds-column-width                │
```

**Truncated with Ellipsis**:
```
│ very-long-resource-group-name-that-exc...                              │
```

**Truncated with Full Text on Selection**:
```
│ ▸ very-long-resource-group-name-that-exceeds-column-width              │
│   ┌────────────────────────────────────────────────────────────────┐   │
│   │ Full name: very-long-resource-group-name-that-exceeds-column-  │   │
│   │            width                                               │   │
│   └────────────────────────────────────────────────────────────────┘   │
```

### Color Coding for Quick Scanning

**Resource Types**:
- `VirtualMachine` - Cyan
- `StorageAccount` - Yellow
- `LoadBalancer` - Green
- `NetworkInterface` - Blue
- `PublicIPAddress` - Magenta
- `KeyVault` - Red

**Status Indicators**:
- `● Running` - Green
- `● Stopped` - Red
- `● Starting` - Yellow
- `● Deallocated` - Gray
- `● Failed` - Red (bold)

---

## Loading States & Feedback

### Loading Indicators

**Global Loading (Header)**:
```
│ aztui > Subscriptions                                            [⟳]    │
```

**Inline Loading (List)**:
```
│ ▸ Production Subscription               [Loading...]                   │
│   Development Subscription              Enabled                         │
```

**Full-Screen Loading (Initial Load)**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│                                                                         │
│                          Loading Subscriptions...                       │
│                                                                         │
│                                  ⟳                                      │
│                                                                         │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

### Progress Indicators

**Determinate Progress**:
```
│ Fetching resources... [████████████░░░░░░░░] 60% (120/200)             │
```

**Indeterminate Progress**:
```
│ Authenticating... [⠋]                                                   │
│ Authenticating... [⠙]                                                   │
│ Authenticating... [⠹]                                                   │
│ Authenticating... [⠸]                                                   │
```

### Success Feedback

**Toast Notification**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│                          ┌──────────────────────┐                       │
│                          │ ✓ Resource started   │                       │
│                          └──────────────────────┘                       │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Inline Success**:
```
│ ▸ vm-web-01                                 ● Running  ✓ Started        │
```

### Error Feedback

**Modal Error Dialog**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│                    ┌────────────────────────────────┐                   │
│                    │ ✗ Error                        │                   │
│                    │                                │                   │
│                    │ Failed to start resource       │                   │
│                    │                                │                   │
│                    │ Reason: Insufficient quota     │                   │
│                    │                                │                   │
│                    │ [R]etry  [C]ancel              │                   │
│                    └────────────────────────────────┘                   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Inline Error**:
```
│ ▸ vm-web-01                                 ● Failed  ✗ Start failed    │
```

---

## Context-Aware Help System

### Help Overlay (Press `?`)

**Subscription List View**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│                          Subscription List Help                         │
├─────────────────────────────────────────────────────────────────────────┤
│ NAVIGATION                                                              │
│   ↑↓ / jk        Move selection up/down                                │
│   g / G          Jump to first/last subscription                       │
│   Enter          View resource groups in subscription                  │
│                                                                         │
│ ACTIONS                                                                 │
│   r              Refresh subscription list                             │
│   /              Filter subscriptions                                  │
│   Ctrl+R         Force refresh (bypass cache)                          │
│                                                                         │
│ GLOBAL                                                                  │
│   Ctrl+P         Open command palette                                  │
│   ?              Toggle this help                                      │
│   q              Quit application                                      │
│                                                                         │
│ Press Esc to close                                                      │
└─────────────────────────────────────────────────────────────────────────┘
```

**Resource List View**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│                          Resource List Help                             │
├─────────────────────────────────────────────────────────────────────────┤
│ NAVIGATION                                                              │
│   ↑↓ / jk        Move selection up/down                                │
│   g / G          Jump to first/last resource                           │
│   Enter          View resource details                                 │
│   Esc            Back to resource groups                               │
│                                                                         │
│ ACTIONS                                                                 │
│   s              Start resource (VM, App Service)                      │
│   t              Stop resource                                         │
│   r              Restart resource                                      │
│   d              Delete resource (with confirmation)                   │
│   l              View logs                                             │
│   m              View metrics                                          │
│   /              Filter resources                                      │
│                                                                         │
│ Press Esc to close                                                      │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Confirmation Dialogs

### Delete Confirmation
```
┌─────────────────────────────────────────────────────────────────────────┐
│                                                                         │
│                    ┌────────────────────────────────┐                   │
│                    │ ⚠ Confirm Deletion             │                   │
│                    │                                │                   │
│                    │ Are you sure you want to       │                   │
│                    │ delete this resource?          │                   │
│                    │                                │                   │
│                    │ Name: vm-web-01                │                   │
│                    │ Type: VirtualMachine           │                   │
│                    │                                │                   │
│                    │ This action cannot be undone.  │                   │
│                    │                                │                   │
│                    │ Type 'delete' to confirm:      │                   │
│                    │ > _                            │                   │
│                    │                                │                   │
│                    │ [Enter] Confirm  [Esc] Cancel  │                   │
│                    └────────────────────────────────┘                   │
│                                                                         │
└─────────────────────────────────────────────────────────────────────────┘
```

**Features**:
- **Explicit confirmation**: Type `delete` to confirm
- **Resource details**: Show what will be deleted
- **Warning indicator**: `⚠` symbol
- **Clear actions**: Enter to confirm, Esc to cancel

---

## Filter Mode

### Filter Input
```
┌─────────────────────────────────────────────────────────────────────────┐
│ aztui > Resources                                        Filter: vm     │
├─────────────────────────────────────────────────────────────────────────┤
│ Filter: vm_                                                             │
├─────────────────────────────────────────────────────────────────────────┤
│ TYPE              NAME          LOCATION    STATUS                      │
│ ▸ VirtualMachine  vm-web-01     eastus      ● Running                  │
│   VirtualMachine  vm-web-02     eastus      ● Running                  │
│                                                                         │
│ [2 resources, 6 filtered out]                                           │
├─────────────────────────────────────────────────────────────────────────┤
│ Type to filter, Esc to clear, Enter to apply                           │
└─────────────────────────────────────────────────────────────────────────┘
```

**Features**:
- **Live filtering**: Results update as you type
- **Filter indicator**: Shows active filter in breadcrumb
- **Filtered count**: Shows how many items are hidden
- **Clear filter**: Esc to clear and show all

### Advanced Filter
```
┌─────────────────────────────────────────────────────────────────────────┐
│                          Advanced Filter                                │
├─────────────────────────────────────────────────────────────────────────┤
│ Name:     [vm-web                                    ]                  │
│ Type:     [VirtualMachine ▼                          ]                  │
│ Location: [eastus ▼                                  ]                  │
│ Status:   [Running ▼                                 ]                  │
│ Tags:     [env:production                            ]                  │
│                                                                         │
│ [Apply Filter]  [Clear]  [Cancel]                                       │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Responsive Layout

### Terminal Size Adaptation

**Large Terminal (120x40)**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│ Full multi-column layout with all details                              │
│ TYPE              NAME          LOCATION    STATUS      SIZE      TAGS  │
└─────────────────────────────────────────────────────────────────────────┘
```

**Medium Terminal (80x24)**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│ Reduced columns, essential info only                                    │
│ TYPE              NAME          LOCATION    STATUS                      │
└─────────────────────────────────────────────────────────────────────────┘
```

**Small Terminal (60x20)**:
```
┌─────────────────────────────────────────────────────────────────────────┐
│ Minimal layout, stacked info                                            │
│ NAME                                                                    │
│ vm-web-01 (VirtualMachine)                                              │
│ eastus | ● Running                                                      │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Accessibility Considerations

### Color Blindness Support
- **Don't rely on color alone**: Use symbols (●, ▸, ✓, ✗) in addition to colors
- **High contrast mode**: Option to use high-contrast color scheme
- **Configurable colors**: Allow users to customize colors

### Screen Reader Support
- **Descriptive labels**: All UI elements have clear labels
- **Status announcements**: Announce state changes (loading, error, success)
- **Keyboard-only navigation**: No mouse required

### Low Vision Support
- **Configurable font size**: Respect terminal font settings
- **Clear focus indicators**: High-contrast focus borders
- **Zoom support**: Terminal zoom works correctly

---

## Performance Targets

### Responsiveness
- **Keystroke latency**: < 16ms (60 FPS)
- **View transition**: < 100ms
- **Initial load**: < 2s (with cache)
- **Refresh**: < 5s (depending on Azure API)

### Scalability
- **List rendering**: Handle 10,000+ items with virtual scrolling
- **Memory usage**: < 100MB for typical workload
- **CPU usage**: < 5% idle, < 20% during refresh

---

## Implementation Priorities

### Phase 1: Foundation (Current - Iteration 4)
- ✅ Basic list navigation
- ✅ Selection highlighting
- ✅ Loading indicators
- ✅ Error overlays
- 🔄 View transitions
- 🔄 Breadcrumb navigation

### Phase 2: Core UX (Iterations 5-9)
- Error handling and display
- Resource detail view
- Search and filtering
- Command palette
- Context-aware help

### Phase 3: Advanced Features (Iterations 10-15)
- Multi-pane layout
- Performance optimization
- Resource actions
- Metrics and monitoring
- Configuration management

### Phase 4: Polish (Iterations 16-18)
- Animations and transitions
- Theme customization
- Export and reporting
- Documentation
- Release preparation

---

## Conclusion

This UI/UX design document defines a **keyboard-first, information-dense, highly discoverable** terminal interface for aztui. The design prioritizes:

1. **k9s-like usability**: Vim-style navigation, mnemonic keybindings, multi-pane support
2. **Information density**: Multi-column layouts, smart truncation, inline metrics
3. **Focus clarity**: Clear visual hierarchy, breadcrumbs, status bars
4. **Command discoverability**: Context-aware help, command palette, footer hints
5. **Intentional interactions**: Confirmations, immediate feedback, consistent patterns

The implementation should follow these principles to create a **best-in-class terminal UI** for Azure resource management.
