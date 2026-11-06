    # PE resource tree: RT_MANIFEST (24), executable manifest (1), en-US.
    .section .rsrc$01,"dr"
    .p2align 2
.Lresource_root:
    .long 0, 0
    .short 0, 0, 0, 1
    .long 24, 0x80000000 + (.Lresource_name - .Lresource_root)
.Lresource_name:
    .long 0, 0
    .short 0, 0, 0, 1
    .long 1, 0x80000000 + (.Lresource_language - .Lresource_root)
.Lresource_language:
    .long 0, 0
    .short 0, 0, 0, 1
    .long 1033, .Lresource_data - .Lresource_root
.Lresource_data:
    .rva .Lmanifest
    .long .Lmanifest_end - .Lmanifest
    .long 0, 0
.Lmanifest:
    .ascii "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>"
    .ascii "<assembly xmlns=\"urn:schemas-microsoft-com:asm.v1\" manifestVersion=\"1.0\">"
    .ascii "<dependency><dependentAssembly><assemblyIdentity type=\"win32\" name=\"Microsoft.Windows.Common-Controls\" version=\"6.0.0.0\" processorArchitecture=\"*\" publicKeyToken=\"6595b64144ccf1df\" language=\"*\"/></dependentAssembly></dependency>"
    .ascii "</assembly>"
.Lmanifest_end:
