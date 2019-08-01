use std::*;
use std::array::FixedSizeArray;
use winapi::um::d3d11 as dx11;
use winapi::um::d3d11_1 as dx11_1;
use winapi::shared;
use winapi::shared::dxgi as dxgi;
use winapi::shared::dxgi1_2 as dxgi2;
#[cfg(debug_assertions)]
use winapi::shared::dxgi1_3 as dxgi3;
use winapi::shared::dxgiformat as dxgifmt;
use winapi::um::d3dcommon as dx;
use winapi::ctypes::c_void as c_void;
use winapi::shared::winerror::*;
use winapi::um::unknwnbase::{IUnknown};

#[cfg(debug_assertions)]
use winapi::um::d3d11sdklayers as sdklayers;
#[cfg(debug_assertions)]
use winapi::um::dxgidebug as dxgidbg;

pub struct D3D11Backend<'a> {
    window : Option<&'a super::window::Window>,

    target_feature_level : u32,
    feature_level : dx::D3D_FEATURE_LEVEL,

    device : *mut dx11_1::ID3D11Device1,
    context : *mut dx11_1::ID3D11DeviceContext1,
    dxgi_factory : *mut dxgi2::IDXGIFactory2,
    swap_chain : *mut dxgi2::IDXGISwapChain1,
    backbuffer_format : dxgifmt::DXGI_FORMAT,
    backbuffer_count : u32,
    depthbuffer_format : dxgifmt::DXGI_FORMAT,

    color_view : *mut dx11::ID3D11RenderTargetView,
    depth_view : *mut dx11::ID3D11DepthStencilView,
    color_target : *mut dx11::ID3D11Texture2D,
    depth_target : *mut dx11::ID3D11Texture2D,
    viewport : dx11::D3D11_VIEWPORT,

    initialized : bool
}

impl<'a> Drop for D3D11Backend<'a> {
    fn drop(&mut self) {
        if self.initialized {
            self.cleanup()
        }
    }
}

impl<'a> Default for D3D11Backend<'a> {
    fn default() -> D3D11Backend<'a> {
        D3D11Backend {
            window: None,
            target_feature_level: 0xb000, // 0xb000 = 11.0 0xb100 = 11.1
            feature_level: 0,
            device: ptr::null_mut(),
            context: ptr::null_mut(),
            dxgi_factory: ptr::null_mut(),
            swap_chain: ptr::null_mut(),
            color_view: ptr::null_mut(),
            color_target: ptr::null_mut(),
            depth_view: ptr::null_mut(),
            depth_target: ptr::null_mut(),
            viewport: Default::default(),
            backbuffer_format: dxgifmt::DXGI_FORMAT_B8G8R8A8_UNORM,
            backbuffer_count: 2,
            depthbuffer_format: dxgifmt::DXGI_FORMAT_D32_FLOAT,
            initialized: false
        }
    }
}

impl<'a> D3D11Backend<'a> {
    pub fn init(window: &super::window::Window) -> Result<D3D11Backend, &'static str> {
        let mut backend = D3D11Backend::default();
        backend.window = Some(window);

        backend.create_factory()?;
        backend.create_device()?;
        backend.create_resources()?;

        backend.initialized = true;
        Ok( backend )
    }
    pub fn cleanup(&mut self) {
        self.depth_view = ptr::null_mut();
        self.color_view = ptr::null_mut();
        self.depth_target = ptr::null_mut();
        self.color_target = ptr::null_mut();
        self.swap_chain = ptr::null_mut();
        self.context = ptr::null_mut();

        #[cfg(debug_assertions)]
        {
            let mut d3d_debug : *mut sdklayers::ID3D11Debug = ptr::null_mut();
            let d3d_debug_uuid = <sdklayers::ID3D11Debug as winapi::Interface>::uuidof();
            let res = unsafe { (*self.device).QueryInterface(&d3d_debug_uuid, &mut d3d_debug as *mut *mut _ as *mut *mut _) };
            if res >= S_OK {
                unsafe { (*d3d_debug).ReportLiveDeviceObjects(sdklayers::D3D11_RLDO_SUMMARY) };
            }
        }

        self.device = ptr::null_mut();
        self.dxgi_factory = ptr::null_mut();
        self.initialized = false;
    }
    
    #[cfg(debug_assertions)]
    fn debug_layers_available() -> bool {
        let res = unsafe { dx11::D3D11CreateDevice(
            ptr::null_mut(),
            dx::D3D_DRIVER_TYPE_NULL,
            ptr::null_mut(), 
            dx11::D3D11_CREATE_DEVICE_DEBUG,
            ptr::null(),
            0,
            dx11::D3D11_SDK_VERSION,
            ptr::null_mut(),
            ptr::null_mut(), 
            ptr::null_mut()
        )};
        (res == S_OK || res == S_FALSE) // S_FALSE indicates nonstandard completion..
    }

    fn create_factory(&mut self) -> Result<(), &'static str> {
        let factory_uuid = <dxgi2::IDXGIFactory2 as winapi::Interface>::uuidof();

        let mut debug_dxgi = false;
        #[cfg(debug_assertions)]
        {
            let mut info_queue : *mut dxgidbg::IDXGIInfoQueue = ptr::null_mut();
            let info_queue_uuid = <dxgidbg::IDXGIInfoQueue as winapi::Interface>::uuidof();
            let mut res = unsafe { dxgi3::DXGIGetDebugInterface1(0, &info_queue_uuid, &mut info_queue as  *mut *mut _ as *mut *mut _) };
            if res >= S_OK {
                debug_dxgi = true;

                res = unsafe { dxgi3::CreateDXGIFactory2(
                    dxgi3::DXGI_CREATE_FACTORY_DEBUG, 
                    &factory_uuid,
                     &mut self.dxgi_factory as *mut *mut _ as *mut *mut _
                )};
                if res < S_OK {
                    return Err( "Unable to create DXGI Factory" )
                }
                unsafe {
                    (*info_queue).SetBreakOnSeverity(dxgidbg::DXGI_DEBUG_ALL, dxgidbg::DXGI_INFO_QUEUE_MESSAGE_SEVERITY_CORRUPTION, 1);
                    (*info_queue).SetBreakOnSeverity(dxgidbg::DXGI_DEBUG_ALL, dxgidbg::DXGI_INFO_QUEUE_MESSAGE_SEVERITY_ERROR, 1);
                }

                let mut hide : [dxgidbg::DXGI_INFO_QUEUE_MESSAGE_ID; 1] = [
                    80, // IDXGISwapChain::GetContainingOutput: The swapchain's adapter does not control the output on which the swapchain's window resides
                ];
                let mut filter : dxgidbg::DXGI_INFO_QUEUE_FILTER = Default::default();
                filter.DenyList.NumIDs = 1;
                filter.DenyList.pIDList = hide.as_mut_ptr();
                unsafe { (*info_queue).AddStorageFilterEntries(dxgidbg::DXGI_DEBUG_DXGI, &filter) };
            }
        }
        if !debug_dxgi {
            let res = unsafe { dxgi::CreateDXGIFactory1(&factory_uuid, &mut self.dxgi_factory as *mut *mut _ as *mut *mut _) };
            if res < S_OK {
                return Err( "Unable to create DXGI Factory" )
            }
        }
        Ok(())
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

        let mut dxgi_adapter_ptr : *mut IUnknown = ptr::null_mut();
        let mut adapter_idx = 0;
        let mut adapter_found = false; 
        let mut dxgi_factory6 : *mut shared::dxgi1_6::IDXGIFactory6 = ptr::null_mut();
        let factory6_uuid = <shared::dxgi1_6::IDXGIFactory6 as winapi::Interface>::uuidof();
        unsafe { 
        let res = (*self.dxgi_factory).QueryInterface(&factory6_uuid, &mut dxgi_factory6 as *mut *mut _ as *mut *mut _);
        if res >= S_OK {
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
                    
                    let mut desc: dxgi::DXGI_ADAPTER_DESC1 = mem::zeroed();
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
        if !adapter_found {
            adapter_idx = 0;
            unsafe {
            loop {
                let mut res = (*(self.dxgi_factory as *mut dxgi::IDXGIFactory1)).EnumAdapters1(
                    adapter_idx,
                    &mut dxgi_adapter_ptr as *mut *mut _ as *mut *mut dxgi::IDXGIAdapter1
                );
                if res != S_OK {
                    println!("Enum Adapters returned {}", res);
                }
                let mut desc: dxgi::DXGI_ADAPTER_DESC1 = mem::zeroed();
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
            let mut desc: dxgi::DXGI_ADAPTER_DESC1 = mem::zeroed();
            let res = (*(dxgi_adapter_ptr as *mut dxgi::IDXGIAdapter1)).GetDesc1(&mut desc);
            if res != S_OK {
                return Err("Unable to get Device Info!");
            }
            let desc_str = String::from_utf16_lossy(desc.Description.as_slice());
            println!("Direct3D Adapter {}: VID: {} PID: {} MEM: {} - {}", adapter_idx, desc.VendorId, desc.DeviceId, desc.DedicatedVideoMemory, desc_str);
            }
        }

        let mut creation_flags = dx11::D3D11_CREATE_DEVICE_BGRA_SUPPORT;

        #[cfg(debug_assertions)]
        {
            if D3D11Backend::debug_layers_available() {
                creation_flags |= dx11::D3D11_CREATE_DEVICE_DEBUG;
            } else {
                println!("WARNING: SDK Layers not available!");
            }
        }
        let mut device : *mut dx11::ID3D11Device = ptr::null_mut();
        let mut context : *mut dx11::ID3D11DeviceContext = ptr::null_mut();
        let res = unsafe { dx11::D3D11CreateDevice(
            dxgi_adapter_ptr as *mut dxgi::IDXGIAdapter, 
            dx::D3D_DRIVER_TYPE_UNKNOWN, 
            ptr::null_mut(), 
            creation_flags, 
            feature_levels.as_ptr(), 
            fl_count, 
            dx11::D3D11_SDK_VERSION, 
            &mut device as *mut *mut _, 
            &mut self.feature_level as *mut _, 
            &mut context as *mut *mut _)
        };

        if res < S_OK {
           return Err( "Unable to create D3D11 Device!" )
        } 

        #[cfg(debug_assertions)]
        {
            let mut d3d11_debug : *mut sdklayers::ID3D11Debug = ptr::null_mut();
            let d3d11_debug_uuid = <sdklayers::ID3D11Debug as winapi::Interface>::uuidof();
            let mut res = unsafe { (*device).QueryInterface(&d3d11_debug_uuid, &mut d3d11_debug as *mut *mut _ as *mut *mut _) };
            if res >= S_OK {
                let mut d3d11_info_queue : *mut sdklayers::ID3D11InfoQueue = ptr::null_mut();
                let d3d11_info_queue_uuid = <sdklayers::ID3D11InfoQueue as winapi::Interface>::uuidof();
                res = unsafe { (*d3d11_debug).QueryInterface(&d3d11_info_queue_uuid, &mut d3d11_info_queue as *mut *mut _ as *mut *mut _) };
                if res >= S_OK {
                    unsafe {
                        (*d3d11_info_queue).SetBreakOnSeverity(sdklayers::D3D11_MESSAGE_SEVERITY_CORRUPTION, 1);
                        (*d3d11_info_queue).SetBreakOnSeverity(sdklayers::D3D11_MESSAGE_SEVERITY_ERROR, 1);
                    }
                    let hide: [sdklayers::D3D11_MESSAGE_ID; 1] = [
                        sdklayers::D3D11_MESSAGE_ID_SETPRIVATEDATA_CHANGINGPARAMS
                    ];
                    let mut filter: sdklayers::D3D11_INFO_QUEUE_FILTER = Default::default();
                    filter.DenyList.NumIDs = 1;
                    filter.DenyList.pIDList = hide.as_ptr();
                    unsafe { (*d3d11_info_queue).AddStorageFilterEntries(&filter) };
                }
            }
        }
        let device_uuid = <dx11_1::ID3D11Device1 as winapi::Interface>::uuidof();
        let context_uuid = <dx11_1::ID3D11DeviceContext1 as winapi::Interface>::uuidof();

        let res = unsafe { (*device).QueryInterface(&device_uuid, &mut self.device as *mut *mut _ as *mut *mut _) };
        if res < S_OK {
            return Err("Unable to get device interface!");
        }
        let res = unsafe { (*context).QueryInterface(&context_uuid, &mut self.context as *mut *mut _ as *mut *mut _) };
        if res < S_OK {
            return Err("Unable to get context interface!");
        }
        Ok(())
    }

    fn create_resources(&mut self) -> Result<(), &'static str> {
        let null_views: *mut dx11::ID3D11RenderTargetView = ptr::null_mut();
        unsafe { (*self.context).OMSetRenderTargets(1, &null_views, ptr::null_mut()) };

        self.color_target = ptr::null_mut();
        self.depth_target = ptr::null_mut();
        self.color_view = ptr::null_mut();
        self.depth_view = ptr::null_mut();

        unsafe {
            (*self.context).Flush();
        }

        let wnd = match self.window {
            Some(ref w) => w,
            None => return Err("Unable to get reference to Window!")
        };
        if !self.swap_chain.is_null() {
            let res = unsafe { (*self.swap_chain).ResizeBuffers(
                self.backbuffer_count,
                wnd.width,
                wnd.height,
                self.backbuffer_format,
                0
            )};
            if res != S_OK {
                return Err( "SwapChain resize failed!");
            }
        } else {
            let mut swapchain_desc : dxgi2::DXGI_SWAP_CHAIN_DESC1 = Default::default();
            swapchain_desc.Width = wnd.width;
            swapchain_desc.Height = wnd.height;
            swapchain_desc.Format = self.backbuffer_format;
            swapchain_desc.BufferUsage = shared::dxgitype::DXGI_USAGE_RENDER_TARGET_OUTPUT;
            swapchain_desc.BufferCount = self.backbuffer_count;
            swapchain_desc.SampleDesc.Count = 1;
            swapchain_desc.SampleDesc.Quality = 0;
            swapchain_desc.Scaling = dxgi2::DXGI_SCALING_STRETCH;
            swapchain_desc.SwapEffect = dxgi::DXGI_SWAP_EFFECT_FLIP_DISCARD;
            swapchain_desc.AlphaMode = dxgi2::DXGI_ALPHA_MODE_IGNORE;
            swapchain_desc.Flags = 0;

            let mut swapchain_desc_fs : dxgi2::DXGI_SWAP_CHAIN_FULLSCREEN_DESC = Default::default();
            swapchain_desc_fs.Windowed = 1;

            let mut res = unsafe { (*self.dxgi_factory).CreateSwapChainForHwnd(
                self.device as *mut _,
                wnd.handle,
                &swapchain_desc,
                &swapchain_desc_fs,
                ptr::null_mut(),
                &mut self.swap_chain as *mut *mut _
            )};
            if res < S_OK {
                println!("SwapChain error: {}", res);
                return Err("SwapChain creation failed!");
            }

            res = unsafe { (*self.dxgi_factory).MakeWindowAssociation(wnd.handle, 2)}; // DXGI_MWA_NO_ALT_ENTER = 1 << 1
            if res < S_OK {
                println!("Window Association error: {}", res);
                return Err("Window Association failed!");
            }

            let swapchain_uuid = <dx11::ID3D11Texture2D as winapi::Interface>::uuidof();
            res = unsafe { (*self.swap_chain).GetBuffer(0, &swapchain_uuid, &mut self.color_target as *mut *mut _ as *mut *mut _) };
            if res < S_OK {
                println!("GetBuffer error: {}", res);
                return Err("Unable to create render target!");
            }
            let mut render_target_view_desc : dx11::D3D11_RENDER_TARGET_VIEW_DESC = Default::default();
            render_target_view_desc.Format = self.backbuffer_format;
            render_target_view_desc.ViewDimension = dx11::D3D11_RTV_DIMENSION_TEXTURE2D;
            res = unsafe { (*self.device).CreateRenderTargetView(
                self.color_target as *mut _,
                &render_target_view_desc,
                &mut self.color_view as *mut *mut _
            )};
            if res < S_OK {
                println!("CreateRenderTargetView error: {}", res);
                return Err("Unable to create render target view!");
            }
            let mut depth_stencil_desc : dx11::D3D11_TEXTURE2D_DESC = Default::default();
            depth_stencil_desc.Format = self.depthbuffer_format;
            depth_stencil_desc.Width = wnd.width;
            depth_stencil_desc.Height = wnd.height;
            depth_stencil_desc.MipLevels = 1;
            depth_stencil_desc.ArraySize = 1;
            depth_stencil_desc.BindFlags = dx11::D3D11_BIND_DEPTH_STENCIL;
            depth_stencil_desc.SampleDesc.Count = 1;
            depth_stencil_desc.SampleDesc.Quality = 0;
            res = unsafe { (*self.device).CreateTexture2D(
                &depth_stencil_desc,
                ptr::null_mut(),
                &mut self.depth_target as *mut *mut _
            ) };
            if res < S_OK {
                println!("CreateTexture2D error: {}", res);
                return Err("Unable to create Depth Stencil attachment!");
            }

            let mut depth_stencil_view_desc : dx11::D3D11_DEPTH_STENCIL_VIEW_DESC = Default::default();
            depth_stencil_view_desc.ViewDimension = dx11::D3D11_DSV_DIMENSION_TEXTURE2D;
            res = unsafe { (*self.device).CreateDepthStencilView(
                self.depth_target as *mut _,
                &depth_stencil_view_desc,
                &mut self.depth_view as *mut *mut _
            )};
            if res < S_OK {
                println!("Create Depth-Stencil view error: {}", res);
                return Err("Unable to create Depth-Stencil view!");
            }

            self.viewport = dx11::D3D11_VIEWPORT { 
                Height: wnd.height as f32,
                Width: wnd.width as f32,
                TopLeftX: 0.0f32,
                TopLeftY: 0.0f32,
                MaxDepth: dx11::D3D11_MAX_DEPTH,
                MinDepth: dx11::D3D11_MIN_DEPTH
            };

        }

        Ok(())
    }

    pub fn present(&mut self) -> Result<(), &'static str> {
        let res = unsafe { (*self.swap_chain).Present(1, 0) };
        unsafe {
            (*self.context).DiscardView(self.color_view as *mut _);            
            (*self.context).DiscardView(self.depth_view as *mut _);
        }
        if res == DXGI_ERROR_DEVICE_REMOVED || res == DXGI_ERROR_DEVICE_RESET {
            #[cfg(debug_assertions)]
            {
                let reason = match res {
                    DXGI_ERROR_DEVICE_REMOVED => unsafe { (*self.device).GetDeviceRemovedReason() },
                    _ => res
                };
                println!("Device lost during present: Reason code 0x{}", reason);
            }

            // todo: handle
        } else {
            if res < S_OK {
                println!("Error during present: {}", res);
                return Err("Present failed");
            }

            if unsafe { (*self.dxgi_factory).IsCurrent() } == 0 {
                self.create_factory()?;
            }
        }
        Ok(())
    }
}