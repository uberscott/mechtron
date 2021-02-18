use std::sync::{Arc, Mutex, MutexGuard};

use no_proto::buffer::NP_Buffer;
use no_proto::error::NP_Error;
use no_proto::memory::NP_Memory_Owned;

use mechtron_core::artifact::Artifact;
use mechtron_core::buffers;
use mechtron_core::configs::{
    Configs, CreateMessageConfig, MessagesConfig, SimConfig, TronConfig,
};
use mechtron_core::core::*;
use mechtron_core::id::{Id, NucleusKey, Revision, StateKey, TronKey};
use mechtron_core::message::{Message, MessageBuilder, MessageKind, Payload, PayloadBuilder};
use mechtron_core::state::{ReadOnlyState, State};

use crate::node::Node;
use crate::nucleus::{Nucleus, TronContext, ApiKind};
use mechtron_core::buffers::{Buffer, Path};
use crate::error::Error;
use std::ops::DerefMut;

pub trait Tron {
    fn create(
        &self,
        info: TronInfo,
        state: Arc<Mutex<State>>,
        create: Arc<Message>,
    ) -> Result<(Option<Vec<MessageBuilder>>), Error>;

    fn update(
        &self,
        phase: &str,
    ) -> Result<
        fn(
            info: TronInfo,
            context: &dyn TronContext,
            state: Arc<Mutex<State>>,
        ) -> Result<Option<Vec<MessageBuilder>>, Error>,
        Error,
    >;

    fn port(
        &self,
        port: &str,
    ) -> Result<
        fn(
            info: TronInfo,
            context: &dyn TronContext,
            state: Arc<Mutex<State>>,
            message: Arc<Message>,
        ) -> Result<Option<Vec<MessageBuilder>>, Error>,
        Error,
    >;

    fn update_phases(&self) -> Phases;
}

pub enum Phases {
    All,
    Some(Vec<String>),
    None,
}

pub struct MessagePort {
    pub receive: fn(
        context: &TronInfo,
        state: &State,
        message: &Message,
    ) -> Result<Option<Vec<MessageBuilder>>, Error>,
}

#[derive(Clone)]
pub struct TronInfo {
    pub key: TronKey,
    pub config: Arc<TronConfig>,
}

impl TronInfo {
    pub fn new(
        key: TronKey,
        tron_config: Arc<TronConfig>,
    ) -> Self {
        TronInfo {
            key: key,
            config: tron_config,
        }
    }


}



pub struct TronShell {
    pub tron: Box<dyn Tron>,
}

impl TronShell {
    pub fn new(tron: Box<dyn Tron>) -> Self {
        TronShell { tron: tron }
    }

    fn from(&self, info: TronInfo, context: &dyn TronContext) -> mechtron_core::message::From {
        mechtron_core::message::From {
            tron: info.key.clone(),
            cycle: context.revision().cycle.clone(),
            timestamp: context.timestamp()
        }
    }

    pub fn create(
        &self,
        info: TronInfo,
        context: &dyn TronContext,
        state:  Arc<Mutex<State>>,
        create: Arc<Message>,
    ) -> Result<Option<Vec<Message>>, Error> {

        let mut builders = self.tron.create(info.clone(), state, create)?;
        return self.handle_builders(info.clone(), builders);
    }

    pub fn receive(
        &mut self,
        info: TronInfo,
        context: &dyn TronContext,
        state: Arc<Mutex<State>>,
        message: Arc<Message>,
    ) -> Result<Option<Vec<Message>>, Error> {
        let func = self.tron.port(&"blah")?;
        let builders = func(info.clone(), context, state, message)?;

        return self.handle_builders(info , builders);
    }

    pub fn handle_builders(
        &self,
        info : TronInfo,
        builders: Option<Vec<MessageBuilder>>,
    ) -> Result<Option<Vec<Message>>, Error> {
        /*            match builders {
                       None => Ok(Option::None),
                       Some(builders) =>
                           {
                               let mut rtn = vec!();
                               for mut builder in builders {
                                   builder.from = Option::Some(self.from(info.clone()));
                                   rtn.push(builder.build(&mut context.sys()?.net.id_seq)?);
                               }
                               Ok(Option::Some(rtn))
                           }

                   }
        */
        unimplemented!()
    }
}

pub struct Neutron {}

pub struct NeutronStateInterface {}

impl NeutronStateInterface {
    fn add_tron(&self, state: &mut State, key: &TronKey, kind: u8) -> Result<(), Error> {
        let index = state.data.get_length(&path!("trons"))?;
        let path = Path::new(path!["trons", index.to_string()]);
        key.append(&path.push(path!["id"]), &mut state.meta);
        state.data.set(&path.plus("kind"), kind)?;

        Ok(())
    }

    fn set_tron_name(
        &self,
        state: &mut State,
        name: &str,
        key: &TronKey,
    ) -> Result<(), Error> {
        key.append(&Path::new(path!["tron_names"]), &mut state.meta);
        Ok(())
    }
}

impl Neutron {
    fn init() -> Result<Box<Tron>, Error> {
        Ok(Box::new(Neutron {}))
    }

    pub fn valid_neutron_id(id: Id) -> bool {
        return id.id == 0;
    }

    pub fn create_tron(
        &self,
        info: TronInfo,
        context: &mut dyn TronContext,
        state: Arc<Mutex<State>>,
        create: Arc<Message>,
    ) -> Result<(), Error> {
        let mut neutron_state = state.lock()?;

        let tron_seq_id = neutron_state.data.get::<i64>(&path!["tron_seq_id"] )?;
        let mut tron_seq = neutron_state.data.get::<i64>(&path!["tron_seq"] )?;
        tron_seq = tron_seq+1;
        neutron_state.data.set( &path!["tron_seq"], tron_seq );

        let tron_key = TronKey::new(info.key.nucleus.clone(), Id::new(tron_seq_id,tron_seq));
        let interface = NeutronStateInterface {};
        interface.add_tron(neutron_state.deref_mut(), &tron_key, 0)?;

        let create_meta = &create.payloads[0].buffer;
        if create_meta.is_set::<String>(&path![&"lookup_name"])?
        {
            let name = create_meta.get::<String>(&path![&"lookup_name"])?;
            interface.set_tron_name(neutron_state.deref_mut(), name.as_str(), &tron_key);
        }

        let tron_config = create_meta.get::<String>(&path![&"artifact"])?;
        let tron_config = Artifact::from(&tron_config)?;
        let tron_config = context.configs().tron_config_keeper.get(&tron_config)?;

        let tron_state_artifact = match tron_config.content
        {
            None => CORE_SCHEMA_EMPTY.clone(),
            Some(_) => {
                tron_config.content.as_ref().unwrap().artifact.clone()
            }
        };

        let mut tron_state = Arc::new(Mutex::new(State::new(context.configs(), tron_state_artifact.clone())?));

        {
            let mut tron_state = tron_state.lock()?;

            tron_state.meta.set(&path![&"artifact"], tron_config.source.to());
            tron_state.meta.set(&path![&"creation_timestamp"], context.timestamp());
            tron_state.meta.set(&path![&"creation_cycle"], context.revision().cycle);
        }


        context.create(tron_key,tron_config.source.clone(), tron_state, create );

        Ok(())
    }
}

impl Tron for Neutron {
    fn create(
        &self,
        context: TronInfo,
        state: Arc<Mutex<State>>,
        create: Arc<Message>,
    ) -> Result<Option<Vec<MessageBuilder>>, Error> {
        /*

        let interface = NeutronStateInterface {};

        //neutron adds itself to the tron manifest
        interface.add_tron(state, &context.key, 0)?;
        interface.set_tron_name(state, "neutron", &context.key)?;

        if create.payloads[1].buffer.is_set::<String>(&path![&"nucleus_lookup_name"])?
        {
            // then we need to pass a message to the simtron to add a lookup name for this nucleus
            let mut builder = MessageBuilder::new();
            builder.to_tron_lookup_name = Option::Some("simtron".to_string());
            builder.to_nucleus_lookup_name= Option::Some("simulation".to_string());
            builder.to_phase = Option::Some(0);
            builder.kind = Option::Some(MessageKind::Update);

            let factory = context.configs().buffer_factory_keeper.get(&CORE_SCHEMA_NUCLEUS_LOOKUP_NAME_MESSAGE)?;
            let buffer = factory.new_buffer(Option::None);
            let mut buffer = Buffer::new(buffer);
            let nucleus_lookup_name: String = create.payloads[1].buffer.get(&path!["nucleus_lookup_name"])?;
            buffer.set( &path!["name"],nucleus_lookup_name );
            context.key.nucleus.append( &Path::just("id"), &mut buffer )?;
            let payload = PayloadBuilder{
                buffer: buffer,
                artifact: CORE_SCHEMA_NUCLEUS_LOOKUP_NAME_MESSAGE.clone()
            };
            let payloads = vec!(payload);
            builder.payloads = Option::Some( payloads );

            Ok(Option::Some(vec!(builder)))
        }
        else{
            Ok(Option::None)
        }
          */
        unimplemented!()
    }

    fn update(&self, phase: &str) -> Result<fn(TronInfo, &dyn TronContext, Arc<Mutex<State>>) -> Result<Option<Vec<MessageBuilder>>, Error>, Error> {
        unimplemented!()
    }

    fn port(&self, port: &str) -> Result<fn(TronInfo, &dyn TronContext, Arc<Mutex<State>>, Arc<Message>) -> Result<Option<Vec<MessageBuilder>>, Error>, Error> {
        unimplemented!()
    }


    fn update_phases(&self) -> Phases {
        Phases::None
    }
}

pub struct CreatePayloadsBuilder {
    pub constructor_artifact: Artifact,
    pub meta: Buffer,
    pub constructor: Buffer,
}

impl CreatePayloadsBuilder {
    pub fn new<'configs> (
        configs: &'configs Configs,
        tron_config: &TronConfig,
    ) -> Result<Self, Error> {

        /*
        let meta_factory = configs.buffer_factory_keeper.get(&CORE_CREATE_META)?;
        let mut meta = Buffer::new(meta_factory.new_buffer(Option::None));
        meta.set(&path![&"artifact"], tron_config.source.to())?;
        let (constructor_artifact, constructor) =
            CreatePayloadsBuilder::constructor(configs, tron_config)?;
        Ok(CreatePayloadsBuilder {
            meta: meta,
            constructor_artifact: constructor_artifact,
            constructor: constructor,
        })
         */
        unimplemented!()
    }
    pub fn set_sim_id(&mut self, sim_id: &Id) -> Result<(), Error> {
        sim_id.append(&Path::just("sim_id"), &mut self.constructor)?;
        Ok(())
    }

    pub fn set_lookup_name(&mut self, lookup_name: &str) -> Result<(), Error> {
        self.meta.set(&path![&"lookup_name"], lookup_name)?;
        Ok(())
    }

    pub fn set_sim_config(&mut self, sim_config: &SimConfig) -> Result<(), Error> {
        self.constructor
            .set(&path!["sim_config_artifact"], sim_config.source.to())?;
        Ok(())
    }

    fn constructor(
        configs: &'static Configs,
        tron_config: &TronConfig,
    ) -> Result<(Artifact, Buffer), Error> {
        /*
        if (&tron_config.messages).is_some() && (&tron_config.messages).unwrap().create.is_some() {
            let constructor_artifact = tron_config.messages.unwrap().create.unwrap().artifact.clone();
            let factory = configs.buffer_factory_keeper.get(&constructor_artifact)?;
            let constructor = factory.new_buffer(Option::None);
            let constructor = Buffer::new(constructor);

            Ok((constructor_artifact, constructor))
        } else {
            let constructor_artifact = CORE_SCHEMA_EMPTY.clone();
            let factory = configs.buffer_factory_keeper.get(&CORE_SCHEMA_EMPTY)?;
            let constructor = factory.new_buffer(Option::None);
            let constructor = Buffer::new(constructor);
            Ok((constructor_artifact, constructor))
        }
         */
        unimplemented!()
    }

    pub fn payloads<'configs>(configs: &'configs Configs, builder: CreatePayloadsBuilder) -> Vec<Payload> {
        let meta_artifact = CORE_CREATE_META.clone();
        vec![
            Payload {
                artifact: meta_artifact,
                buffer: builder.meta.read_only(),
            },
            Payload {
                artifact: builder.constructor_artifact,
                buffer: builder.constructor.read_only(),
            },
        ]
    }
}


pub fn init_tron(config: &TronConfig) -> Result<Box<dyn Tron>, Error> {

    let rtn: Box<Tron> = match config.kind.as_str() {
        "neutron" => Neutron::init()? as Box<Tron>,
        _ => return Err(format!("we don't have a tron of kind {}", config.kind).into()),
    };

    Ok(rtn)
}
