#include "GFSDK_SSAO.h"

struct __declspec(dllexport) HBAO
{
    GFSDK_SSAO_CustomHeap heap;
    GFSDK_SSAO_InputData_D3D11 input;
    GFSDK_SSAO_Context_D3D11 *context;
    GFSDK_SSAO_Parameters parameters;
    GFSDK_SSAO_Output_D3D11 output;

    HBAO(
        ID3D11Device *device,
        ID3D11ShaderResourceView *depthView,
        ID3D11RenderTargetView *renderView,
        float projection[16]);

    int RenderAO(ID3D11DeviceContext *context);
};