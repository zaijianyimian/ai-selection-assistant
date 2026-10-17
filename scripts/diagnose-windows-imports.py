"""Inspect direct PE imports after an abnormal Windows test-loader exit."""
import ctypes
import pathlib
import struct

output = []
def log(*parts):
    line = ' '.join(map(str, parts)); print(line, flush=True); output.append(line)

kernel = ctypes.WinDLL('kernel32', use_last_error=True)
kernel.LoadLibraryW.argtypes = [ctypes.c_wchar_p]
kernel.LoadLibraryW.restype = ctypes.c_void_p
kernel.GetProcAddress.argtypes = [ctypes.c_void_p, ctypes.c_char_p]
kernel.GetProcAddress.restype = ctypes.c_void_p
kernel.FreeLibrary.argtypes = [ctypes.c_void_p]
kernel.GetModuleFileNameW.argtypes = [ctypes.c_void_p, ctypes.c_wchar_p, ctypes.c_uint]

for binary in pathlib.Path('src-tauri/target/debug/deps').glob('ai_selection_assistant_lib-*.exe'):
    log('BINARY', binary)
    data = binary.read_bytes()
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    sections, optional_size = struct.unpack_from('<H12xH', data, pe + 6)
    optional = pe + 24
    directories = optional + (112 if struct.unpack_from('<H', data, optional)[0] == 0x20b else 96)
    section_table = optional + optional_size
    def offset(rva):
        for i in range(sections):
            size, address, raw_size, raw = struct.unpack_from('<IIII', data, section_table + 40*i + 8)
            if address <= rva < address + max(size, raw_size): return raw + rva - address
        return rva
    def string(at): return data[at:data.index(0, at)].decode('ascii')
    imports = offset(struct.unpack_from('<I', data, directories + 8)[0])
    for entry in range(imports, len(data), 20):
        lookup, _, _, name, first = struct.unpack_from('<IIIII', data, entry)
        if not name: break
        dll = string(offset(name))
        handle = kernel.LoadLibraryW(dll)
        loaded_path = ctypes.create_unicode_buffer(32768)
        if handle: kernel.GetModuleFileNameW(handle, loaded_path, len(loaded_path))
        log('DLL', dll, loaded_path.value if handle else 'FAILED', ctypes.get_last_error() if not handle else '')
        if not handle: continue
        thunk = offset(lookup or first)
        for at in range(thunk, len(data), 8):
            value = struct.unpack_from('<Q', data, at)[0]
            if not value: break
            if value >> 63:
                ordinal = value & 0xffff
                address = kernel.GetProcAddress(handle, ctypes.cast(ctypes.c_void_p(ordinal), ctypes.c_char_p))
                if not address: log('MISSING ORDINAL', dll, ordinal)
            else:
                function = string(offset(value) + 2)
                if not kernel.GetProcAddress(handle, function.encode('ascii')): log('MISSING FUNCTION', dll, function)
        kernel.FreeLibrary(handle)
pathlib.Path('windows-loader-diagnostics.txt').write_text('\n'.join(output), encoding='utf-8')
