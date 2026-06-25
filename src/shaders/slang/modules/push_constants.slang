module push_constants;

[[vk::push_constant]] 
public cbuffer PushConstants {
	public float4x4 model;
	public uint tex0;
	public uint tex1;
	public uint tex2;
	public uint tex3;
	public uint tex4;
	public uint has_parallax;
	public uint is_instanced;
};