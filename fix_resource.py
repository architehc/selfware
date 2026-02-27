import os
import re

def fix_file(filepath, replacements):
    with open(filepath, 'r') as f:
        content = f.read()
    for old, new in replacements:
        content = content.replace(old, new)
    with open(filepath, 'w') as f:
        f.write(content)

# Fix errors in all resource files
for f in os.listdir('src/resource'):
    if not f.endswith('.rs'): continue
    path = os.path.join('src/resource', f)
    fix_file(path, [
        ('crate::error::', 'crate::errors::'),
        ('use sysinfo::{DiskExt, System, SystemExt};', 'use sysinfo::{System, Disks};'),
        ('use sysinfo::{System, SystemExt};', 'use sysinfo::System;'),
        ('metrics::gauge!', '// metrics::gauge!'),
    ])

# memory.rs specific fixes
fix_file('src/resource/memory.rs', [
    ('use tracing::{debug, info, warn};', 'use tracing::{debug, info, warn, error};'),
    ('priority_threshold: crate::Priority', 'priority_threshold: u8'),
    ('priority_threshold: crate::Priority::Low', 'priority_threshold: 1'),
    ('priority_threshold: crate::Priority::Normal', 'priority_threshold: 2'),
])

# disk.rs specific fixes
disk_fixes = [
    ('let system = System::new_all();\n        \n        // Find the disk containing checkpoints\n        for disk in system.disks() {', 
     'let disks = Disks::new_with_refreshed_list();\n        \n        // Find the disk containing checkpoints\n        for disk in disks.list() {'),
    ('for disk in system.disks() {', 'for disk in disks.list() {'),
    ('fs::write(&new_path, compressed).await', 'fs::write(&new_path, &compressed).await'),
]
fix_file('src/resource/disk.rs', disk_fixes)

# gpu.rs specific fixes
gpu_fixes = [
    ('available > required_memory * 1.5', 'available as f64 > required_memory as f64 * 1.5'),
    ('available > required_memory * 0.6', 'available as f64 > required_memory as f64 * 0.6'),
]
fix_file('src/resource/gpu.rs', gpu_fixes)

# mod.rs specific fixes
fix_file('src/resource/mod.rs', [
    ('pub use quotas::{ResourceQuotas, AdaptiveQuotas};', 'pub use quotas::AdaptiveQuotas; use crate::config::ResourceQuotas;'),
])

