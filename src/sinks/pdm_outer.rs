use crate::sinks::{prelude::*, utils::formatter::gen_fmt_dat};
use wp_model_core::model::fmt_def::TextFmt;
use wp_model_core::raw::RawData;
use wp_connector_api::SinkResult;

use super::utils::formatter::fds_fmt_proc;

pub trait TDMDataAble {
    fn cov_data(&self, tdo: DataRecord) -> SinkResult<RawData>;
    fn gen_data(&self, data: FmtFieldVec) -> SinkResult<RawData>;
}

impl TDMDataAble for TextFmt {
    fn cov_data(&self, tdo: DataRecord) -> SinkResult<RawData> {
        fds_fmt_proc(*self, tdo)
    }
    fn gen_data(&self, data: FmtFieldVec) -> SinkResult<RawData> {
        gen_fmt_dat(*self, data)
    }
}
