pub mod d3d11 {

use std::array::FixedSizeArray; 
use winapi::shared::windef::HWND;
use winapi::um::d3d11 as dx11;
use winapi::shared;
use winapi::shared::dxgi as dxgi;
use winapi::um::d3dcommon as dx;
use winapi::ctypes::c_void as c_void;
use winapi::shared::winerror::{ S_OK };

pub struct D3D11Backend {
    window: *mut HWND,
    fb_width: u32,
    fb_height: u32,

    target_feature_level: u32,
    feature_level: dx::D3D_FEATURE_LEVEL,

    device: *mut dx11::ID3D11Device,
    context: *mut dx11::ID3D11DeviceContext,
    swap_chain: *mut shared::dxgi1_2::IDXGISwapChain1,

    color_view: *mut dx11::ID3D11RenderTargetView,
    depth_view: *mut dx11::ID3D11RenderTargetView,
    color_target: *mut dx11::ID3D11Texture2D,
    depth_target: *mut dx11::ID3D11Texture2D,
    viewport: *mut dx11::D3D11_VIEWPORT
}

impl Default for D3D11Backend {
    fn default() -> D3D11Backend {
        D3D11Backend {
            window: std::ptr::null_mut(),
            fb_width: 0,
            fb_height: 0,
            target_feature_level: 0xb000, // 0xb000 = 11.0 0xb100 = 11.1
            feature_level: 0,
            device: std::ptr::null_mut(),
            context: std::ptr::null_mut(),
            swap_chain: std::ptr::null_mut(),
            color_view: std::ptr::null_mut(),
            color_target: std::ptr::null_mut(),
            depth_view: std::ptr::null_mut(),
            depth_target: std::ptr::null_mut(),
            viewport: std::ptr::null_mut()
        }
    }
}

impl D3D11Backend {
    pub fn init(&mut self, window: *mut HWND, width: u32, height: u32) {
        self.window = window;
        self.fb_width = std::cmp::max(width, 1);
        self.fb_height = std::cmp::max(height, 1);

        self.create_device();
        self.create_resources();
    }

    fn create_device(&mut self) {
        let feature_levels: [dx::D3D_FEATURE_LEVEL; 2] = [
            dx::D3D_FEATURE_LEVEL_11_1 as u32,
            dx::D3D_FEATURE_LEVEL_11_0 as u32
        ];

        let mut fl_count: u32 = 0;
        for fl in feature_levels.iter() {
            if *fl < self.target_feature_level {
                break;
            }
            fl_count += 1;
        }

        if fl_count == 0 {
            panic!("Target Feature Level is too high!");
        }

        let dxgi_factory: *mut dxgi::IDXGIFactory = std::ptr::null_mut();
        let factory_uuid = <dxgi::IDXGIFactory as winapi::Interface>::uuidof();
        unsafe { 
            let res = dxgi::CreateDXGIFactory(&factory_uuid, &mut (dxgi_factory as *mut c_void)); 
            if res != S_OK {
                panic!("Unable to create DXGI Factory");
            }
        }

        let dxgi_adapter: *mut dxgi::IDXGIAdapter1 = std::ptr::null_mut();
        let dxgi_factory6 = dxgi_factory as *mut shared::dxgi1_6::IDXGIFactory6;
        let factory6_uuid = <shared::dxgi1_6::IDXGIFactory6 as winapi::Interface>::uuidof();
        let mut adapter_idx = 0;
        loop {
            
                let mut res = unsafe { shared::dxgi1_6::IDXGIFactory6::EnumAdapterByGpuPreference(
                    &*dxgi_factory6,
                    adapter_idx,
                    shared::dxgi1_6::DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                    &factory6_uuid,
                    &mut (dxgi_adapter as *mut c_void)
                ) };
                if res == S_OK {
                    let mut desc: dxgi::DXGI_ADAPTER_DESC1 = unsafe { std::mem::zeroed() };
                    res = unsafe { (*dxgi_adapter).GetDesc1(&mut desc) };
                    if res != S_OK {
                        continue; // or throw?
                    }
                    if desc.Flags & dxgi::DXGI_ADAPTER_FLAG_SOFTWARE != 0 {
                        continue; // skip software renderer
                    }
                    let desc_str = String::from_utf16_lossy(desc.Description.as_slice());
                    println!("Direct3D Adapter {}: VID: {} PID: {} MEM: {} - {}", adapter_idx, desc.VendorId, desc.DeviceId, desc.DedicatedVideoMemory, desc_str);
                    break;
                }
            adapter_idx += 1;
        }

        let creation_flags = dx11::D3D11_CREATE_DEVICE_BGRA_SUPPORT;
        let res = unsafe { dx11::D3D11CreateDevice(
            dxgi_adapter as *mut dxgi::IDXGIAdapter, 
            dx::D3D_DRIVER_TYPE_UNKNOWN, 
            std::ptr::null_mut(), 
            creation_flags, 
            feature_levels.as_ptr(), 
            fl_count, 
            dx11::D3D11_SDK_VERSION, 
            &mut self.device, 
            &mut self.feature_level, 
            &mut self.context)
        };
        if res != S_OK {
            panic!("Unable to create D3D11 Device!");
        }
    }

    fn create_resources(&mut self) {
        let null_views: *mut dx11::ID3D11RenderTargetView = std::ptr::null_mut();
        unsafe { (*self.context).OMSetRenderTargets(1, &null_views, std::ptr::null_mut()) };

        
    }
}

}