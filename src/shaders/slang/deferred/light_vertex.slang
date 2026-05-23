float4 main(uint id : SV_VertexId) : SV_Position {
	float2 uv = float2((id << 1) & 2, id & 2) / 2.0;
	float4 vtxPos = float4(uv.x * 2 - 1, -uv.y * 2 + 1, 0.0f, 1.0f);
	return vtxPos;
}