use futures_io::{AsyncBufRead, AsyncRead};
use pyo3::{exceptions::PyValueError, prelude::*, types::PyBytes};
use std::{
    cmp, io,
    pin::Pin,
    task::{Context, Poll},
};

pub struct PyBytesReader<'py> {
    inner: Bound<'py, PyAny>,
    content_length: usize,
    content_length_read: usize,
    inner_buffer: Vec<u8>,
}

impl<'py> PyBytesReader<'py> {
    const DEFAULT_CHUNK_SIZE: usize = 1024;

    pub fn new(py_bytes_reader: Bound<'py, PyAny>) -> PyResult<Self> {
        let content_length_attr = py_bytes_reader.getattr("_content_length")?;
        let content_length = content_length_attr.extract::<usize>().or_else(|_| {
            // NOTE: The stubs for `_content_length` indicate it is supposed to be an `int`, but in
            // actuality it is a `str`.
            content_length_attr.extract::<String>().and_then(|attr| {
                attr.parse::<usize>()
                    .map_err(|err| PyErr::new::<PyValueError, _>(err.to_string()))
            })
        })?;
        Ok(Self {
            inner: py_bytes_reader,
            content_length,
            content_length_read: 0,
            inner_buffer: Vec::with_capacity(0),
        })
    }

    pub fn content_length_remaining(&self) -> usize {
        self.content_length - self.content_length_read
    }
}

impl AsyncRead for PyBytesReader<'_> {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let self_mut = self.get_mut();
        let amt = cmp::min(buf.len(), self_mut.content_length_remaining());
        let read = self_mut.inner.call_method1("read", (amt,))?;
        let bytes = read
            .downcast::<PyBytes>()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?
            .as_bytes();
        buf[..amt].copy_from_slice(&bytes[..amt]);
        self_mut.content_length_read += amt;
        Poll::Ready(Ok(amt))
    }
}

impl AsyncBufRead for PyBytesReader<'_> {
    fn poll_fill_buf(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        let self_mut = self.get_mut();
        let amt = cmp::min(
            PyBytesReader::DEFAULT_CHUNK_SIZE,
            self_mut.content_length_remaining(),
        );
        let read = self_mut.inner.call_method1("read", (amt,))?;
        let bytes = read
            .downcast::<PyBytes>()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err.to_string()))?
            .as_bytes();
        self_mut.inner_buffer = Vec::from(bytes);
        Poll::Ready(Ok(&self_mut.inner_buffer))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        let self_mut = self.get_mut();
        self_mut.content_length_read += amt;
    }
}
