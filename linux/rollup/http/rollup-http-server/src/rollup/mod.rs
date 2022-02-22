/* Copyright 2021 Cartesi Pte. Ltd.
 *
 * Licensed under the Apache License, Version 2.0 (the "License"); you may not
 * use this file except in compliance with the License. You may obtain a copy of
 * the License at http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
 * WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
 * License for the specific language governing permissions and limitations under
 * the License.
 */

/// Implements Rust api to use Linux rollup device

/// Rollup device driver path
pub const ROLLUP_DEVICE_NAME: &str = "/dev/rollup";

use serde::{Deserialize, Serialize};
use std::os::unix::prelude::RawFd;

mod bindings;
pub use bindings::CARTESI_ROLLUP_ADDRESS_SIZE;
pub use bindings::CARTESI_ROLLUP_ADVANCE_STATE;
pub use bindings::CARTESI_ROLLUP_INSPECT_STATE;


pub const REQUEST_TYPE_ADVANCE_STATE: &str = "advance_state";
pub const REQUEST_TYPE_INSPECT_STATE: &str = "inspect_state";

#[derive(Debug, Default)]
pub struct RollupError {
    message: String,
}

impl RollupError {
    pub fn new(message: &str) -> Self {
        RollupError {
            message: String::from(message),
        }
    }
}

impl std::fmt::Display for RollupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "rollup error: {}", &self.message)
    }
}

impl std::error::Error for RollupError {}

#[derive(Debug, Default, Clone, PartialEq, Eq, Copy)]
pub struct RollupFinish {
    pub accept_previous_request: bool,
    pub next_request_type: i32,
    pub next_request_payload_length: i32,
}

impl From<RollupFinish> for bindings::rollup_finish {
    fn from(other: RollupFinish) -> Self {
        bindings::rollup_finish {
            next_request_type: other.next_request_type,
            accept_previous_request: other.accept_previous_request,
            next_request_payload_length: other.next_request_payload_length,
        }
    }
}

impl From<&mut RollupFinish> for bindings::rollup_finish {
    fn from(other: &mut RollupFinish) -> Self {
        bindings::rollup_finish {
            next_request_type: other.next_request_type,
            accept_previous_request: other.accept_previous_request,
            next_request_payload_length: other.next_request_payload_length,
        }
    }
}

impl From<bindings::rollup_finish> for RollupFinish {
    fn from(other: bindings::rollup_finish) -> Self {
        RollupFinish {
            next_request_type: other.next_request_type,
            accept_previous_request: other.accept_previous_request,
            next_request_payload_length: other.next_request_payload_length,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvanceMetadata {
    pub msg_sender: String,
    pub epoch_index: u64,
    pub input_index: u64,
    pub block_number: u64,
    pub timestamp: u64,
}

impl From<bindings::rollup_input_metadata> for AdvanceMetadata {
    fn from(other: bindings::rollup_input_metadata) -> Self {
        let mut address = "0x".to_string();
        address.push_str(&hex::encode(&other.msg_sender));
        AdvanceMetadata {
            input_index: other.input_index,
            epoch_index: other.epoch_index,
            timestamp: other.timestamp,
            block_number: other.block_number,
            msg_sender: address,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdvanceRequest {
    pub metadata: AdvanceMetadata,
    pub payload: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectRequest {
    pub payload: String,
}

pub enum RollupRequest {
    Inspect(InspectRequest),
    Advance(AdvanceRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InspectReport {
    pub reports: Vec<Report>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub payload: String
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voucher {
    pub address: String,
    pub payload: String
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub payload: String,
}

pub fn rollup_finish_request(
    fd: RawFd,
    finish: &mut RollupFinish,
    accept: bool
) -> Result<(), Box<dyn std::error::Error>> {
    let mut finish_c = Box::new(bindings::rollup_finish::from(&mut *finish));

    log::debug!("writing rollup finish request, yielding");
    let res = unsafe { bindings::rollup_finish_request(fd as i32, finish_c.as_mut(), accept) };
    if res != 0 {
        log::error!("failed to write finish request, IOCTL error {}", res);
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_FINISH returned error {}",
            res
        ))));
    }
    *finish = RollupFinish::from(*finish_c);
    log::debug!("finish request written to rollup device: {:#?}", &finish);
    Ok(())
}

pub fn rollup_read_advance_state_request(
    fd: RawFd,
    finish: &mut RollupFinish,
) -> Result<AdvanceRequest, Box<dyn std::error::Error>> {
    let mut finish_c = Box::new(bindings::rollup_finish::from(&mut *finish));
    let mut bytes_c = Box::new(bindings::rollup_bytes {
        data: std::ptr::null::<::std::os::raw::c_uchar>() as *mut ::std::os::raw::c_uchar,
        length: 0,
    });
    let mut input_metadata_c = Box::new(bindings::rollup_input_metadata {
        msg_sender: Default::default(),
        block_number: 0,
        timestamp: 0,
        epoch_index: 0,
        input_index: 0,
    });
    let res = unsafe {
        bindings::rollup_read_advance_state_request(
            fd as i32,
            finish_c.as_mut(),
            bytes_c.as_mut(),
            input_metadata_c.as_mut(),
        )
    };

    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_READ_ADVANCE_STATE returned error {}",
            res
        ))));
    }

    if bytes_c.length == 0 {
        return Err(Box::new(RollupError::new(
            "read zero size from advance state request ",
        )));
    }

    let mut payload: Vec<u8> = Vec::with_capacity(bytes_c.length as usize);
    unsafe {
        std::ptr::copy(bytes_c.data, payload.as_mut_ptr(), bytes_c.length as usize);
        payload.set_len(bytes_c.length as usize);
    }
    let result = AdvanceRequest {
        metadata: AdvanceMetadata::from(*input_metadata_c),
        payload: "0x".to_string() + &hex::encode(&payload),
    };
    *finish = RollupFinish::from(*finish_c);
    Ok(result)
}

pub fn rollup_read_inspect_state_request(
    fd: RawFd,
    finish: &mut RollupFinish,
) -> Result<InspectRequest, Box<dyn std::error::Error>> {
    let mut finish_c = Box::new(bindings::rollup_finish::from(&mut *finish));
    let mut bytes_c = Box::new(bindings::rollup_bytes {
        data: std::ptr::null::<::std::os::raw::c_uchar>() as *mut ::std::os::raw::c_uchar,
        length: 0,
    });
    let res = unsafe {
        bindings::rollup_read_inspect_state_request(fd as i32, finish_c.as_mut(), bytes_c.as_mut())
    };
    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_READ_INSPECT_STATE returned error {}",
            res
        ))));
    }
    if bytes_c.length == 0 {
        return Err(Box::new(RollupError::new(
            "read zero size from inspect state request ",
        )));
    }

    let mut payload: Vec<u8> = Vec::with_capacity(bytes_c.length as usize);
    unsafe {
        std::ptr::copy(bytes_c.data, payload.as_mut_ptr(), bytes_c.length as usize);
        payload.set_len(bytes_c.length as usize);
    }
    let result = InspectRequest {
        payload: "0x".to_string() + &hex::encode(&payload),

    };
    *finish = RollupFinish::from(*finish_c);
    Ok(result)
}

pub fn rollup_write_notices(
    fd: RawFd,
    notice: &mut Notice,
) -> Result<u64, Box<dyn std::error::Error>> {
    print_notice(notice);
    let binary_payload = match hex::decode(&notice.payload[2..]) {
        Ok(payload) => payload,
        Err(_err) => {
            return Err(Box::new(RollupError::new(&format!(
                "Error decoding notice payload, payload must be in Ethereum hex binary format"
            ))));
        }
     };
    let mut buffer: Vec<u8> = Vec::with_capacity(binary_payload.len());
    let mut bytes_c = Box::new(bindings::rollup_bytes {
        data: buffer.as_mut_ptr() as *mut ::std::os::raw::c_uchar,
        length: binary_payload.len() as u64,
    });
    let mut notice_index: std::os::raw::c_ulong = 0;
    let res = unsafe {
        std::ptr::copy(
            binary_payload.as_ptr(),
            buffer.as_mut_ptr(),
            binary_payload.len(),
        );
        bindings::rollup_write_notices(fd as i32, bytes_c.as_mut(), &mut notice_index)
    };
    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_WRITE_NOTICE returned error {}",
            res
        ))));
    } else {
        log::debug!("notice with id {} successfully written!", notice_index);
    }
    Ok(notice_index as u64)
}

pub fn rollup_write_voucher(
    fd: RawFd,
    voucher: &mut Voucher,
) -> Result<u64, Box<dyn std::error::Error>> {
    print_voucher(voucher);
    let binary_payload = match hex::decode(&voucher.payload[2..]) {
        Ok(payload) => payload,
        Err(_err) => {
            return Err(Box::new(RollupError::new(&format!(
                "Error decoding voucher payload, payload must be in Ethereum hex binary format"
            ))));
        }
    };
    let mut buffer: Vec<u8> = Vec::with_capacity(binary_payload.len());
    let mut bytes_c = Box::new(bindings::rollup_bytes {
        data: buffer.as_mut_ptr() as *mut ::std::os::raw::c_uchar,
        length: binary_payload.len() as u64,
    });

    let mut address_c = match hex::decode(&voucher.address[2..]) {
        Ok(res) => res,
        Err(e) => {
            return Err(Box::new(RollupError::new(&format!(
                "address not valid: {}",
                e
            ))));
        }
    };

    let mut voucher_index: std::os::raw::c_ulong = 0;
    let res = unsafe {
        std::ptr::copy(
            binary_payload.as_ptr(),
            buffer.as_mut_ptr(),
            binary_payload.len(),
        );
        bindings::rollup_write_vouchers(
            fd as i32,
            address_c.as_mut_ptr(),
            bytes_c.as_mut(),
            &mut voucher_index,
        )
    };
    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_WRITE_VOUCHER returned error {}",
            res
        ))));
    } else {
        log::debug!("voucher with id {} successfully written!", voucher_index);
    }

    Ok(voucher_index as u64)
}

pub fn rollup_write_report(fd: RawFd, report: &Report) -> Result<(), Box<dyn std::error::Error>> {
    print_report(report);
    let binary_payload = match hex::decode(&report.payload[2..]) {
        Ok(payload) => payload,
        Err(_err) => {
            return Err(Box::new(RollupError::new(&format!(
                "Error decoding report payload, payload must be in Ethereum hex binary format"
            ))));
        }
    };
    let mut buffer: Vec<u8> = Vec::with_capacity(binary_payload.len());
    let mut bytes_c = Box::new(bindings::rollup_bytes {
        data: buffer.as_mut_ptr() as *mut ::std::os::raw::c_uchar,
        length: binary_payload.len() as u64,
    });
    let res = unsafe {
        std::ptr::copy(
            binary_payload.as_ptr(),
            buffer.as_mut_ptr(),
            binary_payload.len(),
        );
        bindings::rollup_write_reports(fd as i32, bytes_c.as_mut())
    };
    if res != 0 {
        return Err(Box::new(RollupError::new(&format!(
            "IOCTL_ROLLUP_WRITE_REPORT returned error {}",
            res
        ))));
    } else {
        log::debug!("report successfully written!");
    }
    Ok(())
}

pub fn print_address(address: &str) {
    if address.starts_with("0x") {
        log::debug!("{}", address);
    } else {
        log::debug!("0x{}", address);
    }
}

pub fn print_advance(advance: &AdvanceRequest) {
    log::debug!("advance: {{\n\tmsg_sender: ");
    print_address(&advance.metadata.msg_sender);
    log::debug!(
        "\tblock_number: {}\n\ttimestamp: {}\n\tepoch_index: {}\n\tinput_index: {}\n}}",
        advance.metadata.block_number,
        advance.metadata.timestamp,
        advance.metadata.epoch_index,
        advance.metadata.input_index
    );
}

pub fn print_inspect(inspect: &InspectRequest) {
    log::debug!(
        "Inspect: {{\n\tlength: {} payload: {}\n}}",
        inspect.payload.len(),
        inspect.payload
    );
}

pub fn print_notice(notice: &Notice) {
    log::debug!(
        "Notice: {{\n\tlength: {} payload: {}\n}}",
        notice.payload.len(),
        notice.payload
    );
}

pub fn print_voucher(voucher: &Voucher) {
    log::debug!("voucher: {{\n\taddress:");
    print_address(&voucher.address);
    log::debug!(
        "\tlength: {} payload: {}\n}}",
        voucher.payload.len(),
        voucher.payload
    );
}

pub fn print_report(report: &Report) {
    log::debug!(
        "report: {{\n\tlength: {} payload: {}\n}}",
        report.payload.len(),
        report.payload
    );
}
