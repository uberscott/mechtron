use core::cell::RefCell;
use core::default::Default;
use core::fmt::Debug;
use core::option::Option;
use core::option::Option::{None, Some};
use core::result::Result;
use core::result::Result::{Err, Ok};
use std::collections::HashMap;
use std::sync::{Arc, MutexGuard};

use mechtron_core::buffers::ReadOnlyBuffer;
use mechtron_core::id::Id;
use mechtron_core::message::{Cycle, DeliveryMoment, MechtronLayer, Message, MessageBuilder, MessageKind, Payload};
use mechtron_core::state::{ReadOnlyState, ReadOnlyStateMeta, State, StateMeta};
use mechtron_core::util::PongPayloadBuilder;

use crate::error::Error;
use crate::mechtron::{MechtronKernel, TronInfo, TronShellState};
use crate::nucleus::MechtronShellContext;

pub struct MechtronShell {
    pub tron: Box<dyn MechtronKernel>,
    pub info: TronInfo,
    pub outbound: RefCell<Vec<Message>>,
    pub panic: bool,
}


impl MechtronShell {
    pub fn new(tron: Box<dyn MechtronKernel>, info: TronInfo) -> Self {
        MechtronShell {
            tron: tron,
            info: info,
            outbound: RefCell::new(vec!()),
            panic: false,
        }
    }

    fn warn<E: Debug>(&self, error: E)
    {
        println!("WARN: TronShell got unexpected error {:?}", error);
    }

    fn panic<E: Debug>(&self, error: E)
    {
        println!("PANIC: TronShell got unexpected error {:?}", error);
    }

    fn reject(&mut self, message: &Message, reason: &str, context: &dyn MechtronShellContext, layer: MechtronLayer)
    {
        println!("{}", reason);
        // we unrwrap it because if REJECT message isn't available, then nothing should work
        let message = message.reject(self.from(context, layer), reason, context.seq(), context.configs()).unwrap();
        self.send(message)
    }

    fn ok(&mut self, message: &Message, ok: bool, context: &dyn MechtronShellContext, layer: MechtronLayer)
    {
        // we unrwrap it because if REJECT message isn't available, then nothing should work
        let message = message.ok(self.from(context, layer), ok, context.seq(), context.configs()).unwrap();
        self.send(message)
    }

    fn respond(&mut self, message: &Message, payloads: Vec<Payload>, context: &dyn MechtronShellContext, layer: MechtronLayer)
    {
        let message = message.respond(self.from(context, layer), payloads, context.seq());
        self.send(message);
    }

    fn from(&self, context: &dyn MechtronShellContext, layer: MechtronLayer) -> mechtron_core::message::From {
        mechtron_core::message::From {
            tron: self.info.key.clone(),
            cycle: context.revision().cycle.clone(),
            timestamp: context.timestamp(),
            layer: layer,
        }
    }

    fn send(&mut self, message: Message)
    {
        self.outbound.borrow_mut().push(message);
    }

    pub fn flush(&self) -> Vec<Message>
    {
        self.outbound.replace(Default::default())
    }

    pub fn create(
        &mut self,
        create: &Message,
        context: &dyn MechtronShellContext,
        state: &mut State,
    ) {
        match self.create_result(create, context, state)
        {
            Ok(_) => {}
            Err(error) => {
                self.panic(error);
                state.set_taint(true);
            }
        }
    }

    fn create_result(
        &mut self,
        create: &Message,
        context: &dyn MechtronShellContext,
        state: &mut State,
    ) -> Result<(), Error> {
        if state.is_tainted()?
        {
            return Err("mechtron state is tainted".into());
        }
        let mut builders = self.tron.create(self.info.clone(), context, state, create)?;
        self.handle(builders, context)?;

        Ok(())
    }

    pub fn extra(
        &mut self,
        message: &Message,
        context: &dyn MechtronShellContext,
        state: Arc<ReadOnlyState>,
    )
    {
        match self.extra_result(message, context, state)
        {
            Ok(_) => {}
            Err(err) => {
                self.warn(err);
            }
        }
    }

    fn extra_result(
        &mut self,
        message: &Message,
        context: &dyn MechtronShellContext,
        state: Arc<ReadOnlyState>,
    ) -> Result<(), Error>
    {
        if state.is_tainted()?
        {
            return Err("mechtron state is tainted".into());
        }
        println!("entered EXTRA");
        match message.to.layer
        {
            MechtronLayer::Shell => {
                match message.to.port.as_str() {
                    "ping" => {
                        println!("PING!!!");
                        self.respond(message, vec!(PongPayloadBuilder::new(context.configs()).unwrap()), context, MechtronLayer::Shell);
                    }
                    "pong" => {
                        println!("PONG!!!");
                    }

                    _ => {
                        self.reject(message, format!("TronShell has no extra port: {}", message.to.port.clone()).as_str(), context, MechtronLayer::Shell);
                    }
                };
            }
            MechtronLayer::Kernel => {
                let bind = context.configs().binds.get(&self.info.config.bind.artifact).unwrap();
                match bind.message.extra.contains_key(&message.to.port)
                {
                    true => {
                        let func = self.tron.extra(&message.to.port);
                        match func {
                            Ok(func) => {
                                let builders = func(self.info.clone(), context, &state, message)?;
                                self.handle(builders, context);
                            }
                            Err(e) => {
                                self.warn(e);
                            }
                        }
                    }
                    false => {
                        self.reject(message, format!("extra cyclic port '{}' does not exist on this mechtron", message.to.port).as_str(), context, MechtronLayer::Shell);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn inbound(
        &mut self,
        messages: &Vec<Arc<Message>>,
        context: &dyn MechtronShellContext,
        state: &mut MutexGuard<State>,
    ) {
        match self.inbound_result(messages, context, state)
        {
            Ok(_) => {}
            Err(error) => {
                self.panic(error);
                state.set_taint(true);
            }
        }
    }

    pub fn inbound_result(
        &mut self,
        messages: &Vec<Arc<Message>>,
        context: &dyn MechtronShellContext,
        state: &mut MutexGuard<State>,
    ) -> Result<(), Error> {
        if state.is_tainted()?
        {
            return Err("mechtron state is tainted".into());
        }

        let bind = context.configs().binds.get(&self.info.config.bind.artifact).unwrap();
        let mut hash = HashMap::new();
        for message in messages
        {
            if !hash.contains_key(&message.to.port)
            {
                hash.insert(message.to.port.clone(), vec!());
            }
            let messages = hash.get_mut(&message.to.port).unwrap();
            messages.push(message.clone());
        }
        let mut ports = vec!();
        for port in hash.keys()
        {
            ports.push(port);
        }

        ports.sort();
        for port in ports
        {
            let messages = hash.get(port).unwrap();
            match bind.message.inbound.contains_key(port)
            {
                true => {
                    let func = self.tron.port(port);
                    match func
                    {
                        Ok(func) => {
                            let builders = func(self.info.clone(), context, state, messages)?;
                            self.handle(builders, context)?;
                        }
                        Err(e) => {
                            self.panic(e);
                        }
                    }
                }
                false => {
                    for message in messages {
                        self.reject(message, format!("mechtron {} does not have an inbound port {}", self.info.config.source.to(), port).as_str(), context, MechtronLayer::Shell);
                    }
                }
            }
        }
        Ok(())
    }

    fn handle(
        &self,
        builders: Option<Vec<MessageBuilder>>,
        context: &dyn MechtronShellContext,
    ) -> Result<(), Error> {
        println!("HANDLE");
        match builders {
            None => Ok(()),
            Some(builders) => {
                for mut builder in builders
                {
                    println!("builder: {:?}", builder.kind.as_ref().unwrap().clone());
                    builder.from = Option::Some(self.from(context, MechtronLayer::Kernel));

                    if builder.to_nucleus_lookup_name.is_some()
                    {
                        let nucleus_id = context.lookup_nucleus(&builder.to_nucleus_lookup_name.unwrap())?;
                        builder.to_nucleus_id = Option::Some(nucleus_id);
                        builder.to_nucleus_lookup_name = Option::None;
                    }

                    if builder.to_tron_lookup_name.is_some()
                    {
                        let nucleus_id = builder.to_nucleus_id;
                        match nucleus_id {
                            None => {
                                // do nothing. builder.build() will panic for us
                            }
                            Some(nucleus_id) => {
                                let tron_key = context.lookup_mechtron(&nucleus_id, &builder.to_tron_lookup_name.unwrap().as_str())?;
                                builder.to_tron_id = Option::Some(tron_key.mechtron);
                                builder.to_tron_lookup_name = Option::None;
                            }
                        }
                    }

                    if builder.kind.as_ref().is_some() && builder.kind.as_ref().unwrap().clone() == MessageKind::Api
                    {
                        // handle API message
                        self.handle_api_call(builder, context)?;
                    } else {
                        let message = builder.build(context.seq().clone())?;
                        self.outbound.borrow_mut().push(message);
                    }
                }

                Ok(())
            }
        }
    }


    fn handle_api_call(
        &self,
        mut builder: MessageBuilder,
        context: &dyn MechtronShellContext,
    ) -> Result<(), Error>
    {
        println!("handle_api_call!");
        builder.to_cycle_kind = Option::Some(Cycle::Present);
        builder.to_nucleus_id = Option::Some(self.info.key.nucleus.clone());
        builder.to_tron_id = Option::Some(self.info.key.mechtron.clone());
        builder.to_delivery = Option::Some(DeliveryMoment::Phasic);
        builder.to_port = Option::Some("api".to_string());
        builder.to_layer = Option::Some(MechtronLayer::Shell);
        builder.to_phase = Option::Some("default".to_string());
        builder.from = Option::Some(self.from(context, MechtronLayer::Kernel));
        let message = builder.build(context.seq().clone())?;
        let bind = context.configs().binds.get(&self.info.config.bind.artifact).unwrap();

        println!("got a message with payloads: {}", message.payloads.len());
        let api = message.payloads[0].buffer.get::<String>(&path!["api"])?;

        match api.as_str() {
            "neutron_api" => {
                // need some test to make sure this is actually a neutron
                if !bind.kind.eq("Neutron")
                {
                    self.panic(format!("attempt for non Neutron to access neutron_api {}", bind.kind));
                } else {
                    let call = message.payloads[0].buffer.get::<String>(&path!["call"])?;
                    match call.as_str() {
                        "create_mechtron" => {
                            println!("READY TO CREATE A MECHTRON!");

println!("ARTIFACT FOR PAYLOAD: {}", message.payloads[1].schema.to());
println!("TAINT {}", message.payloads[1].buffer.get::<bool>(&path!["taint"])?);
                            // now get the state of the mechtronmessage.payloads
                            let new_mechtron_state = State::new_from_meta(context.configs(), message.payloads[1].buffer.copy_to_buffer())?;

                            println!("got state!");
                            // very wasteful to be cloning the bytes here...
                            let create_message = message.payloads[2].buffer.read_bytes().to_vec();
                            let create_message = Message::from_bytes(create_message, context.configs())?;
                            println!("create_message. meta {}", create_message.meta.is_some() );
                            println!("sending to context.neutron_api_create()!");
                            context.neutron_api_create(new_mechtron_state, create_message);
                        }

                        _ => { return Err(format!("we don't have an api {} call {}", api, call).into()); }
                    }
                }
            }
            _ => { return Err(format!("we don't have an api {}", api).into()); }
        }

        Ok(())
    }
}
