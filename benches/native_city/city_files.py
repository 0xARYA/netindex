"""Immutable City benchmark files and Linux page-cache checks."""

import ctypes
import hashlib
import mmap
import os


def sha256_file(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verify_residency(path, cache):
    if cache not in ("cold", "warm"):
        raise ValueError(f"unknown cache state: {cache}")

    with path.open("rb", buffering=0) as stream:
        if cache == "cold":
            os.posix_fadvise(stream.fileno(), 0, 0, os.POSIX_FADV_DONTNEED)
        else:
            while stream.read(1024 * 1024):
                pass

        size = os.fstat(stream.fileno()).st_size
        pages = (size + mmap.PAGESIZE - 1) // mmap.PAGESIZE
        vector = (ctypes.c_ubyte * pages)()
        with mmap.mmap(stream.fileno(), 0, access=mmap.ACCESS_COPY) as view:
            address = ctypes.addressof(ctypes.c_char.from_buffer(view))
            libc = ctypes.CDLL(None, use_errno=True)
            if (
                libc.mincore(ctypes.c_void_p(address), ctypes.c_size_t(size), vector)
                != 0
            ):
                raise OSError(ctypes.get_errno(), "mincore")

    resident = sum(value & 1 for value in vector)
    expected = 0 if cache == "cold" else pages
    if resident != expected:
        raise ValueError(f"{path.name}: {resident}/{pages} pages resident ({cache})")

    return {"resident": resident, "total": pages}
