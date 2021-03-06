use no_proto::buffer::NP_Buffer;
use no_proto::NP_Factory;

use crate::artifact::Artifact;
use std::error::Error;
use no_proto::memory::{NP_Memory_Owned, NP_Memory, NP_Mem_New};
use no_proto::pointer::{NP_Scalar, NP_Value};

pub trait BufferFactories
{
    fn create_buffer(&self, artifact: &Artifact) ->Result<NP_Buffer<NP_Memory_Owned>,Box<dyn Error>>;
    fn create_buffer_from_array(&self, artifact: &Artifact, array: Vec<u8> ) ->Result<NP_Buffer<NP_Memory_Owned>,Box<dyn Error>>;
    fn create_buffer_from_buffer(&self, artifact: &Artifact, buffer: NP_Buffer<NP_Memory_Owned> ) ->Result<NP_Buffer<NP_Memory_Owned>,Box<dyn Error>>;
    fn get_buffer_factory(&self, artifact: &Artifact) ->Option<&'static NP_Factory<'static>>;
}


pub fn get<'get, X: 'get,M: NP_Memory + Clone + NP_Mem_New>(buffer:&'get NP_Buffer<M>, path: &[&str]) -> Result<X, Box<dyn Error>> where X: NP_Value<'get> + NP_Scalar<'get> {
    match buffer.get::<X>(path)
    {
        Ok(option)=>{
            match option{
                Some(rtn)=>Ok(rtn),
                None=>Err(format!("expected a value for {}", path[path.len()-1] ).into())
            }
        },
        Err(e)=>Err(format!("could not get {}",cat(path)).into())
    }
}



pub fn set<'get, X: 'get,M: NP_Memory + Clone + NP_Mem_New>(buffer:&'get mut NP_Buffer<M>, path: &[&str], value: X) -> Result<bool, Box<dyn Error>> where X: NP_Value<'get> + NP_Scalar<'get> {
    match buffer.set::<X>(path, value)
    {
        Ok(option)=>{
            match option{
                Some(rtn)=>Ok(rtn),
                None=>Err(format!("expected a value for {}", path[path.len()-1] ).into())
            }
        },
        Err(e)=>Err(format!("could not set {}",cat(path)).into())
    }
}


