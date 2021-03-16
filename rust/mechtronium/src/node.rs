use std::alloc::System;
use std::borrow::{Borrow, BorrowMut};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::sync::{Arc, RwLock, Weak, Mutex};

use wasmer::{Cranelift, JIT, Module, Store};

use mechtron_common::artifact::Artifact;
use mechtron_common::core::*;
use mechtron_common::id::{Id, IdSeq, MechtronKey};
use mechtron_common::message::{Message, MessageKind, MechtronLayer, To, Cycle, DeliveryMoment, MessageTransport};

use crate::artifact::MechtroniumArtifactRepository;
use crate::cache::{Cache, default_cache};
use crate::error::Error;
use crate::nucleus::{Nuclei, NucleiContainer, Nucleus};
use crate::router::{HasNucleus, LocalRouter, NetworkRouter, InternalRouter, SharedRouter};
use crate::simulation::Simulation;
use crate::mechtron::CreatePayloadsBuilder;
use mechtron_common::configs::{SimConfig, Configs, Keeper, NucleusConfig, Parser};
use crate::cluster::Cluster;
use crate::network::{NodeRouter, Wire, Connection, Route, WireListener, ExternalRoute, ReportUniqueSeqPayload, RelayPayload, NodeFind};
use std::fmt;


pub struct Node{
    pub id: RefCell<Option<Id>>,
    pub seq: RefCell<Option<Arc<IdSeq>>>,
    pub local: RefCell<Option<Arc<Local>>>,
    pub kind: NodeKind,
    pub cache: Arc<Cache>,
    pub router: Arc<NodeRouter>,
    connection_transactions: Mutex<HashMap<Id,Arc<Connection>>>
}


impl Node {


    pub fn new(kind: NodeKind, cache: Option<Arc<Cache>>) -> Self {

        let cache = match cache{
            None => {
                default_cache()
            }
            Some(cache) => cache
        };


        let mut rtn = Node {
            kind: kind,
            id: RefCell::new(Option::None),
            seq: RefCell::new(Option::None),
            local: RefCell::new(Option::None),
            cache: cache.clone(),
            router: Arc::new(NodeRouter::new()),
            connection_transactions: Mutex::new(HashMap::new()),
        };

        if rtn.kind.is_central()
        {
           rtn.init_with_sequence(0);
        }

        rtn
    }

    pub fn id(&self)->Id
    {
        self.id.borrow().as_ref().expect("this node has not been initialized.").clone()
    }

    pub fn seq(&self)->Arc<IdSeq>
    {
        self.seq.borrow().as_ref().expect("this node has not been initialized.").clone()
    }

    fn local(&self)->Arc<Local>
    {
        self.local.borrow().as_ref().expect("this node has not been initialized.").clone()
    }

    pub fn is_init(&self)->bool
    {
        self.id.borrow().is_some()
    }

    fn init_with_sequence(&self, seq: i64 )->Result<(),Error>
    {
        if self.id.borrow().is_some()
        {
            return Err("Node id is already set and cannot be modified!".into());
        }
        self.id.replace(Option::Some(Id::new(seq,0)));
        let seq = Arc::new(IdSeq::with_seq_and_start_index(seq,1 ));
        self.seq.replace(Option::Some(seq.clone()));
        self.local.replace(Option::Some(Arc::new(Local::new(self.cache.clone(), seq.clone(), self.router.clone()))));
        self.router.set_node_id(self.id());
        Ok(())
    }


    pub fn shutdown(&self) {}

    pub fn create_sim_from_scratch(&self, config: Arc<SimConfig>) -> Result<Id, Error> {

        if self.local.borrow().is_none()
        {
            return Err("local is none".into())
        }

        let id = self.local.borrow().as_ref().unwrap().sources.create_sim(config)?;

       Ok(id)
    }

    pub fn timestamp(&self)->u64
    {
        // implement later
        0
    }

    pub fn send( &self, message: Message )
    {
        unimplemented!()
     //   self.internal_router.send( Arc::new(message))
    }

}


pub struct Local {
    sources: Arc<Nuclei>,
    router: Arc<dyn Route>,
    seq: Arc<IdSeq>,
    cache: Arc<Cache>
}


impl NucleiContainer for Local
{
    fn has_nucleus(&self, id: &Id) -> bool {
        self.sources.has_nucleus(id)
    }
}

impl  Local{
    fn new(cache: Arc<Cache>, seq: Arc<IdSeq>, router: Arc<dyn Route>) -> Self {
        let rtn = Local {
            sources: Nuclei::new(cache.clone(), seq.clone(), router.clone()),
            router: router,
            seq: seq.clone(),
            cache: cache.clone()
        };

        rtn
    }

    pub fn has_nucleus(&self, id: &Id)->bool
    {
        self.sources.has_nucleus(id)
    }

    pub fn seq(&self)->Arc<IdSeq>
    {
        self.seq.clone()
    }

    pub fn cache(&self)->Arc<Cache>
    {
        self.cache.clone()
    }

    pub fn create_source_nucleus( &self, sim_id: Id, config: Arc<NucleusConfig>, lookup_name: Option<String> )->Result<Id,Error>
    {
        self.sources.create(sim_id, lookup_name, config )
    }
}

impl InternalRouter for Local
{
    fn send(&self, message: Arc<Message>) {
        self.router.relay(message);
    }

    fn receive(&self, message: Arc<Message>) {
        let mut result = self.sources.get( &message.to.tron.nucleus);

        if result.is_err()
        {
            println!("cannot find nucleus with id: {:?}",message.to.tron.nucleus);
        }
        else {
            let mut nucleus = result.unwrap();
            nucleus.intake_message(message);
        }
    }

    fn has_nucleus_local(&self, nucleus: &Id) -> HasNucleus {
        match self.sources.has_nucleus(nucleus)
        {
            true => HasNucleus::Yes,
            false => HasNucleus::No
        }
    }

}


/*
#[derive(Clone)]
pub struct NucleusContext{
    sys: Arc<Node<M>>,
}

impl<M> NucleusContext<M> where M: NodeManager {
    pub fn new(sys: Arc<Node<M>>) -> Self {
        NucleusContext { sys: sys }
    }
    pub fn sys<'get>(&'get self) -> Arc<Node<M>> {
        self.sys.clone()
    }
}
 */


pub struct WasmStuff
{
    pub wasm_store: Arc<Store>,
    pub wasms: Keeper<Module>,
}



impl WireListener for Node
{
    fn describe(&self)->String
    {
        format!("NodeKind: {}",self.kind)
    }

    fn on_wire(&self, wire: Wire, mut connection: Arc<Connection>) -> Result<(), Error> {

println!("on_wire()  {} -> {}",wire,self.kind,);
        match wire{
            Wire::ReportVersion(_)=> {
                // if we have a Node Id, return it
                if self.id.borrow().is_some()
                {
                    connection.to_remote(Wire::ReportNodeId(self.id()));
                }
                else
                {
                    connection.to_remote(Wire::RequestUniqueSeq);
                }
            },
            Wire::RequestUniqueSeq => {

                if self.kind.is_central()
                {
                    connection.to_remote( Wire::ReportUniqueSeq(ReportUniqueSeqPayload{ seq: self.seq().next().id }));
                }
                else {
println!("don't know how to provide a unique sequence... relaying to central");
                    let transaction = self.seq().next();
                    let wire = Wire::Relay(
                            RelayPayload {
                                from: self.id(),
                                to: Id::new(0, 0),
                                wire: Box::new(Wire::RequestUniqueSeq),
                                transaction: transaction.clone(),
                                hops: 0
                            }
                    );
                    {
                        let mut connection_transactions = self.connection_transactions.lock()?;
                        connection_transactions.insert(transaction, connection);
                    }

                    self.router.relay_wire(wire);
                }
            }
            Wire::ReportUniqueSeq(payload) => {
                self.init_with_sequence(payload.seq);
                connection.to_remote( Wire::ReportNodeId(self.id()));
            }
            Wire::ReportNodeId(node_id) => {
                connection.add_found_node(node_id, NodeFind::new(1,u64::MAX));
                self.router.add_external_connection( connection);
            }
            Wire::NodeSearch(search) => {
                let mut search  = search.clone();
                search.hops = search.hops+1;

                // since the request came from this route we know that's where to find it
                connection.add_found_node(search.from.clone(), NodeFind::new(search.hops, self.timestamp() ));
                // we can also say the seeking node cannot be found in that direction
                connection.add_unfound_node(search.seeking_id.clone() );


println!("seeking: {:?} and this is: {:?}",search.seeking_id.clone(), self.id());
                if search.seeking_id == self.id()
                {
println!("~~~~~ FOUND ~~~~~");
                    connection.wire(Wire::Relay(
                        RelayPayload{
                            from: self.id(),
                            to:  search.from.clone(),
                            wire: Box::new(Wire::NodeFound(search)),
                            transaction: self.seq.borrow().as_ref().unwrap().next(),
                            hops: 0
                        }
                        ))?;
                }
                else if( search.hops > 255 )
                {
                    search.reverse(self.id());
                    connection.wire(Wire::Relay(
                        RelayPayload{
                            from: self.id(),
                            to:  search.from.clone(),
                            wire: Box::new(Wire::NodeNotFound(search)),
                            transaction: self.seq.borrow().as_ref().unwrap().next(),
                            hops: 0
                        }
                    ))?;
                }
                else
                {
println!("~~~~~ RELAY NODE SEARCH ~~~~~");
                    self.router.relay_wire( Wire::NodeSearch(search) )?;
                }
            }
            Wire::NodeFound(search) => {
                connection.add_found_node(search.seeking_id.clone(), NodeFind::new(search.hops, self.timestamp() ));
            }
            Wire::MessageTransport(transport) => {
            }
            Wire::Relay(payload) => {
               if payload.hops > 255
               {
                   panic!("Too many payload hops!");
               }
               match *payload.wire{
                   Wire::Relay(_) => {
                       return Err("CANNOT relay a relay".into())
                   }
                   _ => {
                       if payload.to == self.id()
                       {
                           match *payload.wire
                           {
                               Wire::RequestUniqueSeq => {
                                  if self.kind.is_central()
                                  {
                                      let relay = Wire::Relay(
                                          RelayPayload {
                                              to: payload.from.clone(),
                                              from: self.id(),
                                              wire: Box::new(Wire::ReportUniqueSeq(ReportUniqueSeqPayload { seq: self.seq().next().id })),
                                              transaction: payload.transaction.clone(),
                                              hops: payload.hops+1
                                          }
                                      );
                                      self.router.relay_wire(relay)?;
                                  }
                                  else {
                                      println!("RELAY PANIC! only central should be receiving ReportUniqueSeq from Relay")

                                  }
                               },
                               Wire::ReportUniqueSeq(seq_id)=> {
                                   let mut transaction= {let mut connection_transactions = self.connection_transactions.lock()?;
                                   connection_transactions.remove(&payload.transaction)};
                                   if transaction.is_some()
                                   {
                                       let connection = transaction.unwrap();
                                       connection.to_remote(Wire::ReportUniqueSeq(seq_id));
                                   }
                                   else {
                                       println!("cannot find connection transaction {:?}", payload.transaction );
                                   }
                               }
                               Wire::NodeFound(search)=> {
println!("RELAY: Node found");
                                   connection.add_found_node(search.seeking_id.clone() , NodeFind::new(search.hops,self.timestamp()));
                                   self.router.notify_found( &search.seeking_id );

                               }
                               Wire::NodeNotFound(search)=> {

println!("RELAY: Node NOT found")
                               }

                               _ => {
                                   println!("RELAY PANIC! Don't know how to handle {}",payload.wire)
                               }
                           }
                       }
                       else {
                           // it's not meant for us, so let's pass it on...
println!("RELAYING RELAY TO next...");
                           self.router.relay_wire(Wire::Relay(payload));
                       }
                   }
               }

            }

            Wire::Panic(_) => {}
            _ => {
                return Err("don't know how to hanle this Wire.".into());
            }
        }
        Ok(())
    }
}


pub struct Mesh
{
}

pub struct Gateway
{
}

pub struct Client
{
}

pub enum NodeKind
{
    Central(Cluster),
    Server,
    Mesh,
    Gateway,
    Client
}

impl NodeKind
{
    fn create_id(&self)->Option<Id>
    {
       match self{
           NodeKind::Central(cluster) => Option::Some(Id::new(0, 0)),
           _ => Option::None
       }
    }

    fn is_central(&self)->bool
    {
        match self{
            NodeKind::Central(_) => true,
            _ => false
        }
    }
}

impl fmt::Display for NodeKind{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let r = match self {
            NodeKind::Central(_) => {"Central"}
            NodeKind::Server => {"Server"}
            NodeKind::Mesh => {"Mesh"}
            NodeKind::Gateway => {"Gateway"}
            NodeKind::Client => {"Client"}
        };
        write!(f, "{}",r)
    }
}