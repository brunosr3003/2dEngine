# Nuvens do Zone14

Modelos derivados de `/home/brunji/zone14/assets/models/cloud_*.vox`, gerados
originalmente por `zone14/generate_clouds.py`. Cumulus foi reduzido 3×;
small e stratus, 2×. A redução conserva a cor majoritária da superfície de
cada célula, como `zone14/tools/vox_para_csharp.py::reduz`.

O cliente usa o mesher voxel existente, remove faces internas e aplica a
iluminação suave do `zone14/src/Client/BichoGpu.cs::SombraDaNuvem`.
