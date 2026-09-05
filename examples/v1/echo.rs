#![no_std]
#![no_main]

#[link(wasm_import_module = "env")]
unsafe extern "C" {
    fn get_input(ptr: *mut u8, capacity: u32) -> u32;
}
static mut OUTPUT: [u8; 1048577] = [0; 1048577];
#[unsafe(no_mangle)]
pub extern "C" fn hoya_main() -> i32 {
    unsafe {
        let ptr = core::ptr::addr_of_mut!(OUTPUT).cast::<u8>();
        let len = get_input(ptr, 1048576);
        *ptr.add(len as usize) = 0;
        ptr as i32
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
