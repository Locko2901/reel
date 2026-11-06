//! The graphics adapters Windows lists, by maker, so the encoder test can skip a maker with none.

/// PCI vendor ids.
pub const NVIDIA: u32 = 0x10DE;
pub const AMD: u32 = 0x1002;
pub const INTEL: u32 = 0x8086;

/// The vendor ids of the hardware adapters DXGI lists; None where it cannot be asked.
pub fn vendors() -> Option<Vec<u32>> {
    sys::vendors()
}

#[cfg(windows)]
mod sys {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };

    pub fn vendors() -> Option<Vec<u32>> {
        let factory: IDXGIFactory1 = unsafe { CreateDXGIFactory1() }.ok()?;
        let mut out = Vec::new();
        for i in 0u32.. {
            let Ok(adapter) = (unsafe { factory.EnumAdapters1(i) }) else {
                break;
            };
            let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
                continue;
            };
            // Microsoft's Basic Render Driver
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            out.push(desc.VendorId);
        }
        Some(out)
    }
}

#[cfg(not(windows))]
mod sys {
    pub fn vendors() -> Option<Vec<u32>> {
        None
    }
}
