// Adapted from the MIT-licensed `losslessly` crate's MozJPEG coefficient
// transcoder. DCT coefficients are copied verbatim; only entropy coding changes.

use anyhow::{Result, anyhow};
use mozjpeg_sys::*;
use std::mem;
use std::os::raw::c_ulong;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

pub fn optimize(data: &[u8], strip: bool) -> Result<Vec<u8>> {
    let baseline = transcode(data, false, strip)?;
    let progressive = transcode(data, true, strip)?;
    Ok(if progressive.len() < baseline.len() {
        progressive
    } else {
        baseline
    })
}

fn transcode(data: &[u8], progressive: bool, strip: bool) -> Result<Vec<u8>> {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        transcode_unchecked(data, progressive, strip)
    }))
    .map_err(|panic| {
        let message = panic
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| panic.downcast_ref::<&str>().map(|value| value.to_string()))
            .unwrap_or_else(|| "Unknown MozJPEG error".into());
        anyhow!(message)
    })
}

unsafe extern "C-unwind" fn error_exit_panic(cinfo: &mut jpeg_common_struct) {
    let buffer = [0u8; 80];
    if let Some(format) = unsafe { (*cinfo.err).format_message } {
        unsafe { format(cinfo, &buffer) };
    }
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    panic!("libjpeg: {}", String::from_utf8_lossy(&buffer[..length]));
}

struct DecompressGuard(jpeg_decompress_struct);
impl Drop for DecompressGuard {
    fn drop(&mut self) {
        unsafe { jpeg_destroy_decompress(&mut self.0) }
    }
}

struct CompressGuard(jpeg_compress_struct);
impl Drop for CompressGuard {
    fn drop(&mut self) {
        unsafe { jpeg_destroy_compress(&mut self.0) }
    }
}

unsafe fn transcode_unchecked(data: &[u8], progressive: bool, strip: bool) -> Vec<u8> {
    unsafe {
        let copy_option = if strip {
            JCOPY_OPTION_JCOPYOPT_NONE
        } else {
            JCOPY_OPTION_JCOPYOPT_ALL
        };

        let mut source_error: jpeg_error_mgr = mem::zeroed();
        jpeg_std_error(&mut source_error);
        source_error.error_exit = Some(error_exit_panic);
        let mut source = DecompressGuard(mem::zeroed());
        source.0.common.err = &mut source_error;
        jpeg_create_decompress(&mut source.0);
        jpeg_mem_src(&mut source.0, data.as_ptr(), data.len() as c_ulong);
        jcopy_markers_setup(&mut source.0, copy_option);
        jpeg_read_header(&mut source.0, 1);
        let coefficients = jpeg_read_coefficients(&mut source.0);

        let mut destination_error: jpeg_error_mgr = mem::zeroed();
        jpeg_std_error(&mut destination_error);
        destination_error.error_exit = Some(error_exit_panic);
        let mut destination = CompressGuard(mem::zeroed());
        destination.0.common.err = &mut destination_error;
        jpeg_create_compress(&mut destination.0);
        jpeg_copy_critical_parameters(&source.0, &mut destination.0);
        destination.0.optimize_coding = 1;
        if progressive {
            jpeg_simple_progression(&mut destination.0);
        }

        let mut output: *mut u8 = ptr::null_mut();
        let mut output_size: c_ulong = 0;
        jpeg_mem_dest(&mut destination.0, &mut output, &mut output_size);
        jpeg_write_coefficients(&mut destination.0, coefficients);
        jcopy_markers_execute(&mut source.0, &mut destination.0, copy_option);
        jpeg_finish_compress(&mut destination.0);
        jpeg_finish_decompress(&mut source.0);
        if source_error.num_warnings > 0 {
            panic!("Corrupt JPEG data was rejected");
        }

        let result = std::slice::from_raw_parts(output, output_size as usize).to_vec();
        libc::free(output.cast());
        result
    }
}
