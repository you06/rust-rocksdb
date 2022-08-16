// Copyright 2017 PingCAP, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// See the License for the specific language governing permissions and
// limitations under the License.

use crocksdb_ffi::{self, DBEntryType, DBTablePropertiesCollector, DBUserCollectedProperties};
use libc::{c_char, c_int, c_void, size_t};
use std::collections::HashMap;
use std::ffi::CString;
use std::mem;
use std::slice;

/// `TablePropertiesCollector` provides the mechanism for users to collect
/// their own properties that they are interested in. This class is essentially
/// a collection of callback functions that will be invoked during table
/// building. It is constructed with TablePropertiesCollectorFactory. The methods
/// don't need to be thread-safe, as we will create exactly one
/// TablePropertiesCollector object per table and then call it sequentially
pub trait TablePropertiesCollector {
    /// Will be called when a new key/value pair is inserted into the table.
    fn add(&mut self, key: &[u8], value: &[u8], entry_type: DBEntryType, seq: u64, file_size: u64);

    /// Will be called when a table has already been built and is ready for
    /// writing the properties block.
    fn finish(&mut self) -> HashMap<Vec<u8>, Vec<u8>>;

    fn need_compact(&self) -> bool {
        false
    }
}

struct TablePropertiesCollectorHandle<T: TablePropertiesCollector> {
    name: CString,
    rep: T,
}

impl<T: TablePropertiesCollector> TablePropertiesCollectorHandle<T> {
    fn new(name: &str, rep: T) -> TablePropertiesCollectorHandle<T> {
        TablePropertiesCollectorHandle {
            name: CString::new(name).unwrap(),
            rep: rep,
        }
    }
}

extern "C" fn name<T: TablePropertiesCollector>(handle: *mut c_void) -> *const c_char {
    unsafe {
        let handle = &mut *(handle as *mut TablePropertiesCollectorHandle<T>);
        handle.name.as_ptr()
    }
}

extern "C" fn destruct<T: TablePropertiesCollector>(handle: *mut c_void) {
    unsafe {
        Box::from_raw(handle as *mut TablePropertiesCollectorHandle<T>);
    }
}

pub extern "C" fn add<T: TablePropertiesCollector>(
    handle: *mut c_void,
    key: *const u8,
    key_len: size_t,
    value: *const u8,
    value_len: size_t,
    entry_type: c_int,
    seq: u64,
    file_size: u64,
) {
    unsafe {
        let handle = &mut *(handle as *mut TablePropertiesCollectorHandle<T>);
        let key = slice::from_raw_parts(key, key_len);
        let value = slice::from_raw_parts(value, value_len);
        handle
            .rep
            .add(key, value, mem::transmute(entry_type), seq, file_size);
    }
}

pub extern "C" fn finish<T: TablePropertiesCollector>(
    handle: *mut c_void,
    props: *mut DBUserCollectedProperties,
) {
    unsafe {
        let handle = &mut *(handle as *mut TablePropertiesCollectorHandle<T>);
        for (key, value) in handle.rep.finish() {
            crocksdb_ffi::crocksdb_user_collected_properties_add(
                props,
                key.as_ptr(),
                key.len(),
                value.as_ptr(),
                value.len(),
            );
        }
    }
}

pub extern "C" fn need_compact<T: TablePropertiesCollector>(handle: *const c_void) -> bool {
    unsafe {
        let handle = &*(handle as *const TablePropertiesCollectorHandle<T>);
        handle.rep.need_compact()
    }
}

pub unsafe fn new_table_properties_collector<T: TablePropertiesCollector>(
    cname: &str,
    collector: T,
) -> *mut DBTablePropertiesCollector {
    let handle = TablePropertiesCollectorHandle::new(cname, collector);
    crocksdb_ffi::crocksdb_table_properties_collector_create(
        Box::into_raw(Box::new(handle)) as *mut c_void,
        name::<T>,
        destruct::<T>,
        add::<T>,
        finish::<T>,
        need_compact::<T>,
    )
}
