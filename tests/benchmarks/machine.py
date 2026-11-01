"""Process memory and machine details for the benchmark runners, on Linux and macOS."""
import os
import platform
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def extension():
    """The release extension, or STATS_EXTENSION."""
    name = 'libphit.dylib' if sys.platform == 'darwin' else 'libphit.so'
    return Path(os.environ.get('STATS_EXTENSION', ROOT / 'target/release' / name)).resolve()


def memory_command():
    """A prefix that runs a command and reports its peak memory on stderr in the form of macOS /usr/bin/time -l.

    On Linux that is bench/peakmem.c, built and put on the PATH (or named by PEAKMEM):
    cc -O2 -o ~/.local/bin/peakmem bench/peakmem.c
    """
    if sys.platform == 'darwin':
        return ['/usr/bin/time', '-l']
    tool = os.environ.get('PEAKMEM') or shutil.which('peakmem')
    if not tool:
        raise FileNotFoundError('peakmem not found: cc -O2 -o ~/.local/bin/peakmem bench/peakmem.c')
    return [tool, '-l']


def peak_memory(stderr):
    """The peak resident set size and the peak memory footprint, in bytes, from memory_command()'s report.

    The footprint is the process's own allocations without the file pages it maps: macOS's peak memory
    footprint, or the peak of RssAnon on Linux.
    """
    rss = re.search(r'(\d+)\s+maximum resident set size', stderr)
    footprint = re.search(r'(\d+)\s+peak memory footprint', stderr)
    return {'peak_rss_bytes': int(rss[1]) if rss else None,
            'peak_memory_bytes': int(footprint[1]) if footprint else None}


def machine():
    """CPU model, memory, and operating system of this host."""
    if sys.platform == 'darwin':
        cpu = subprocess.check_output(['sysctl', '-n', 'machdep.cpu.brand_string'], text=True).strip()
        memory = int(subprocess.check_output(['sysctl', '-n', 'hw.memsize'], text=True))
        system = platform.platform()
    else:
        model = re.search(r'^model name\s*:\s*(.+)$', Path('/proc/cpuinfo').read_text(), re.M)
        cpu = model[1].strip() if model else platform.processor() or platform.machine()
        memory = 1024 * int(re.search(r'^MemTotal:\s*(\d+)', Path('/proc/meminfo').read_text(), re.M)[1])
        release = re.search(r'^PRETTY_NAME="(.+)"', Path('/etc/os-release').read_text(), re.M)
        system = f'Linux, {release[1] if release else platform.release()} ({platform.machine()})'
    return {'cpu': cpu, 'cpu_count': os.cpu_count(), 'host_memory_bytes': memory, 'platform': system}
