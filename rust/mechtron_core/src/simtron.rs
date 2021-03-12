use core::option::Option;
use core::option::Option::{None, Some};
use core::result::Result;
use core::result::Result::{Err, Ok};
use mechtron::CONFIGS;
use mechtron_common::api::CreateApiCallCreateNucleus;
use mechtron_common::artifact::Artifact;
use mechtron_common::error::Error;
use mechtron_common::message::{Cycle, MechtronLayer, Message, MessageBuilder, MessageKind};
use mechtron_common::state::{ReadOnlyState, State};
use mechtron::mechtron::{Mechtron, Response,  MessageHandler};
use std::sync::MutexGuard;
use mechtron_common::mechtron::Context;
use mechtron::membrane::StateLocker;

pub struct Simtron{
    context: Context,
    state_locker: StateLocker
}

impl Simtron{

    pub fn new(context:Context,state_locker: StateLocker)->Self
    {
        Simtron{
            context: context,
            state_locker: state_locker
        }
    }
}

impl Mechtron for Simtron{



     fn create(&self, create_message: &Message) -> Result<Response, Error>
     {
        let sim_config = match &create_message.meta
        {
            None => {
                return Err("bootstrap meta is not set".into())
            }
            Some(bootstrap_meta) => match bootstrap_meta.get("sim_config")
            {
                None => {
                    return Err("sim_config is not set in bootstrap_meta".into())
                }
                Some(sim_config_artifact) => {
                    let artifact = Artifact::from(sim_config_artifact.as_str())?;
                    CONFIGS.sims.get(&artifact)?
                }
            }
        };

        self.state_locker.state().buffers.get_mut("data").unwrap().set(&path!["config"], sim_config.source.to() )?;


        // now create each of the Nucleus in turn

        let mut builders = vec!();
        for nucleus_ref in &sim_config.nucleus
        {
            let create_api_call_create_nucleus = CreateApiCallCreateNucleus::new(  nucleus_ref.clone() );
            let mut builder = MessageBuilder::new();
            builder.kind = Option::Some(MessageKind::Api);
            builder.to_layer = Option::Some(MechtronLayer::Shell);
            builder.to_nucleus_id=Option::Some(self.context.key.nucleus.clone());
            builder.to_tron_id=Option::Some(self.context.key.mechtron.clone());
            builder.to_cycle_kind=Option::Some(Cycle::Present);
            builder.payloads.replace(Option::Some(CreateApiCallCreateNucleus::payloads(create_api_call_create_nucleus,&CONFIGS)?));
            builders.push( builder );
        }
        Ok(Response::Messages(builders))
    }



    fn update(&self) -> Result<Response, Error> {
        Ok(Response::None)
    }

    fn message(&self, port: &str) -> Result<MessageHandler, Error> {
        Ok(MessageHandler::None)
    }

    fn extra(&self, port: &str) -> Result<MessageHandler, Error> {
        Ok(MessageHandler::None)
    }

    fn state(&self) -> &StateLocker {
        &self.state_locker
    }
}
