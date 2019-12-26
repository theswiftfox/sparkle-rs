#include "GFSDK_SSAO_D3D11_ext.hpp"

#include <stdexcept>
#include <iostream>

HBAO::HBAO(ID3D11Device *device,
           ID3D11ShaderResourceView *depthView,
           ID3D11RenderTargetView *renderView,
           float projection[16])
{
    heap = GFSDK_SSAO_CustomHeap();
    heap.new_ = ::operator new;
    heap.delete_ = ::operator delete;

    auto status = GFSDK_SSAO_CreateContext_D3D11(
        device,
        &context,
        &heap);

    if (status != GFSDK_SSAO_OK)
    {
        throw std::runtime_error("HBAO+ Context init failed");
    }

    input = GFSDK_SSAO_InputData_D3D11();
    input.DepthData.DepthTextureType = GFSDK_SSAO_HARDWARE_DEPTHS;
    input.DepthData.pFullResDepthTextureSRV = depthView;
    input.DepthData.ProjectionMatrix.Data = GFSDK_SSAO_Float4x4(projection);
    input.DepthData.ProjectionMatrix.Layout = GFSDK_SSAO_ROW_MAJOR_ORDER;
    input.DepthData.MetersToViewSpaceUnits = 1.0;

    parameters = GFSDK_SSAO_Parameters();
    parameters.Radius = 2.0f;
    parameters.Bias = 0.1f;
    parameters.PowerExponent = 2.0f;
    parameters.Blur.Enable = true;
    parameters.Blur.Radius = GFSDK_SSAO_BLUR_RADIUS_4;
    parameters.Blur.Sharpness = 4.0f;

    output = GFSDK_SSAO_Output_D3D11();
    output.Blend.Mode = GFSDK_SSAO_OVERWRITE_RGB;
    output.pRenderTargetView = renderView;
}

int HBAO::RenderAO(ID3D11DeviceContext *ctx)
{
    // for (size_t i = 0; i < 4; i++)
    // {
    //     for (size_t j = 0; j < 4; j++)
    //     {
    //         auto idx = 4 * i + j;
    //         std::cout << input.DepthData.ProjectionMatrix.Data.Array[idx] << std::endl;
    //     }
    // }
    return context->RenderAO(ctx, input, parameters, output);
}