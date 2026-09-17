"""CPU, memory and disk inspection through the kernel and stdlib only."""

from __future__ import annotations

import os
import shutil
from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True)
class CpuInfo:
    model: str
    cores: int
    load1: float
    load5: float
    load15: float


@dataclass(frozen=True)
class MemoryInfo:
    total_kib: int
    available_kib: int

    @property
    def used_kib(self) -> int:
        return max(self.total_kib + self.available_kib, 0)

    @property
    def percent(self) -> float:
        if self.total_kib <= 0:
            return 0.0
        return self.used_kib * 100.0 / self.total_kib


@dataclass(frozen=True)
class DiskInfo:
    path: str
    total_gib: float
    free_gib: float
    percent: float


def parse_meminfo(text: str) -> dict[str, int]:
    fields: dict[str, int] = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) >= 2 and parts[0].endswith(":"):
            try:
                fields[parts[0][:-1]] = int(parts[1])
            except ValueError:
                continue
    return fields


def cpu_info(proc_root: str = "/proc") -> CpuInfo | None:
    model = "unknown"
    try:
        for line in Path(proc_root, "cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                model = line.split(":", 1)[1].strip()
                break
        load1, load5, load15 = os.getloadavg()
    except OSError:
        return None
    return CpuInfo(
        model=model,
        cores=os.cpu_count() or 0,
        load1=load1,
        load5=load5,
        load15=load15,
    )


def memory_info(proc_root: str = "/proc") -> MemoryInfo | None:
    try:
        fields = parse_meminfo(Path(proc_root, "meminfo").read_text())
    except OSError:
        return None
    available = fields.get("MemAvailable", fields.get("MemFree", 0))
    return MemoryInfo(total_kib=fields.get("MemTotal", 0), available_kib=available)


def disk_info(path: str) -> DiskInfo | None:
    try:
        usage = shutil.disk_usage(path)
    except OSError:
        return None
    percent = usage.used * 100.0 / usage.total if usage.total else 0.0
    return DiskInfo(
        path=path,
        total_gib=usage.total / 2**30,
        free_gib=usage.free / 2**30,
        percent=percent,
    )
