use std::array::FixedSizeArray;
use winapi::um::d3d11 as dx11;
use winapi::shared;
use winapi::shared::dxgi as dxgi;
use winapi::um::d3dcommon as dx;
use winapi::ctypes::c_void as c_void;
use winapi::shared::winerror::{ S_OK };
use winapi::um::unknwnbase::{IUnknown};

#[derive(Debug)]
pub struct D3D11Backend<'a> {
    window: Option<&'a super::window::Window>,

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

impl<'a> Default for D3D11Backend<'a> {
    fn default() -> D3D11Backend<'a> {
        D3D11Backend {
            window: None,
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

impl<'a> D3D11Backend<'a> {
    pub fn init(window: &super::window::Window) -> Result<D3D11Backend, &'static str> {
        let mut backend = D3D11Backend::default();
        backend.window = Some(window);

        backend.create_device()?;
   //     self.create_resources();

        Ok( backend )
    }

    fn create_device(&mut self) -> Result<(), &'static str> {
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
            return Err( "Target Feature Level is too high!" )
        }

        let mut dxgi_factory_ptr: *mut IUnknown = std::ptr::null_mut();
        let factory_uuid = <dxgi::IDXGIFactory1 as winapi::Interface>::uuidof();
        unsafe { 
            let res = dxgi::CreateDXGIFactory1(&factory_uuid, &mut dxgi_factory_ptr as *mut *mut _ as *mut *mut _);
            if res != S_OK {
                return Err( "Unable to create DXGI Factory" )
            }
        }

        let mut dxgi_adapter_ptr : *mut IUnknown = std::ptr::null_mut();
        let dxgi_factory6 = dxgi_factory_ptr as *mut shared::dxgi1_6::IDXGIFactory6;
        let factory6_uuid = <shared::dxgi1_6::IDXGIFactory6 as winapi::Interface>::uuidof();
        let mut adapter_idx = 0;
        let mut adapter_found = false;
        unsafe { 
        loop {
                let mut res = shared::dxgi1_6::IDXGIFactory6::EnumAdapterByGpuPreference(
                    &(*dxgi_factory6),
                    adapter_idx,
                    shared::dxgi1_6::DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                    &factory6_uuid,
                    &mut dxgi_adapter_ptr as *mut *mut _ as *mut *mut c_void
                );
                if res < 0 {
                    break;
                }
                
                let mut desc: dxgi::DXGI_ADAPTER_DESC1 = std::mem::zeroed();
                res = (*(dxgi_adapter_ptr as *mut dxgi::IDXGIAdapter1)).GetDesc1(&mut desc);
                if res != S_OK {
                    return Err("Unable to get Device Info!");
                }
                if desc.Flags & dxgi::DXGI_ADAPTER_FLAG_SOFTWARE != 0 {
                    adapter_idx += 1;
                    continue; // skip software renderer
                }
                adapter_found = true;
                break;

        }
        }
        if !adapter_found {
            adapter_idx = 0;
            unsafe {
            loop {
                let mut res = (*(dxgi_factory_ptr as *mut dxgi::IDXGIFactory1)).EnumAdapters1(
                    adapter_idx,
                    &mut dxgi_adapter_ptr as *mut *mut _ as *mut *mut dxgi::IDXGIAdapter1
                );
                if res != S_OK {
                    println!("Enum Adapters returned {}", res);
                }
                let mut desc: dxgi::DXGI_ADAPTER_DESC1 = std::mem::zeroed();
                res = (*(dxgi_adapter_ptr as *mut dxgi::IDXGIAdapter1)).GetDesc1(&mut desc);
                if res != S_OK {
                    return Err("Unable to get Device Info!");
                }
                if desc.Flags & dxgi::DXGI_ADAPTER_FLAG_SOFTWARE != 0 {
                    adapter_idx += 1;
                    continue; // skip software renderer
                }
                adapter_found = true;
                break;
            }
            }
        }

        if adapter_found {
            unsafe {
            let mut desc: dxgi::DXGI_ADAPTER_DESC1 = std::mem::zeroed();
            let res = (*(dxgi_adapter_ptr as *mut dxgi::IDXGIAdapter1)).GetDesc1(&mut desc);
            if res != S_OK {
                return Err("Unable to get Device Info!");
            }
            let desc_str = String::from_utf16_lossy(desc.Description.as_slice());
            println!("Direct3D Adapter {}: VID: {} PID: {} MEM: {} - {}", adapter_idx, desc.VendorId, desc.DeviceId, desc.DedicatedVideoMemory, desc_str);
            }
        }

        let creation_flags = dx11::D3D11_CREATE_DEVICE_BGRA_SUPPORT;
        let res = unsafe { dx11::D3D11CreateDevice(
            dxgi_adapter_ptr as *mut dxgi::IDXGIAdapter, 
            dx::D3D_DRIVER_TYPE_UNKNOWN, 
            std::ptr::null_mut(), 
            creation_flags, 
            feature_levels.as_ptr(), 
            fl_count, 
            dx11::D3D11_SDK_VERSION, 
            &mut self.device as *mut *mut _, 
            &mut self.feature_level as *mut _, 
            &mut self.context as *mut *mut _)
        };
        if res != S_OK {
            Err( "Unable to create D3D11 Device!" )
        } else {
            Ok(())
        }
    }

    fn create_resources(&mut self) {
        let null_views: *mut dx11::ID3D11RenderTargetView = std::ptr::null_mut();
        unsafe { (*self.context).OMSetRenderTargets(1, &null_views, std::ptr::null_mut()) };

        
    }
}