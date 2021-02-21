use std::borrow::BorrowMut;
use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::{Arc, RwLock};

use no_proto::NP_Factory;
use semver::Version;
use serde::{Deserialize, Serialize};
use crate::core::*;

use crate::artifact::{Artifact, ArtifactBundle, ArtifactCache, ArtifactRepository, ArtifactYaml };
use crate::error::Error;

pub struct Configs<'config> {
    pub artifacts: Arc<dyn ArtifactCache + Sync + Send>,
    pub schemas: Keeper<NP_Factory<'config>>,
    pub sims: Keeper<SimConfig>,
    pub trons: Keeper<TronConfig>,
    pub nucleus: Keeper<NucleusConfig>,
    pub mechtrons: Keeper<MechtronConfig>
}

impl<'config> Configs<'config> {
    pub fn new(artifact_cache: Arc<dyn ArtifactCache + Sync + Send>) -> Self {
        let mut configs = Configs {
            artifacts: artifact_cache.clone(),
            schemas: Keeper::new(
                artifact_cache.clone(),
                Box::new(NP_Buffer_Factory_Parser),
                Option::None
            ),
            sims: Keeper::new(artifact_cache.clone(), Box::new(SimConfigParser), Option::Some(Box::new(SimConfigArtifactCacher{}))),
            trons: Keeper::new(artifact_cache.clone(), Box::new(TronConfigParser), Option::Some(Box::new(TronConfigArtifactCacher{}))),
            nucleus: Keeper::new(artifact_cache.clone(), Box::new(NucleusConfigParser ), Option::Some(Box::new(NucleusConfigArtifactCacher{}))),
            mechtrons: Keeper::new(
                artifact_cache.clone(),
                Box::new(MechtronConfigParser),
                Option::None
            ),
        };

        configs.cache_core();

        return configs;
    }

    pub fn cache( &mut self, artifact: &Artifact )->Result<(),Error>
    {
        match &artifact.kind{
            None => {
                self.artifacts.cache(artifact)?;
                Ok(())
            }
            Some(kind) => {
                match kind.as_str(){
                    "schema"=>Ok(self.schemas.cache(artifact)?),
                    "tron_config"=>{
                        self.trons.cache(artifact)?;
                        let config = self.trons.get(artifact)?;
                        for artifact in self.trons.get_cacher().as_ref().unwrap().artifacts(config)?
                        {
                            &self.cache(&artifact)?;
                        }
                        Ok(())
                    },
                    "nucleus"=>{
                        self.nucleus.cache(artifact)?;
                        let config = self.nucleus.get(artifact)?;
                        for artifact in self.nucleus.get_cacher().as_ref().unwrap().artifacts(config)?
                        {
                            &self.cache(&artifact)?;
                        }
                        Ok(())
                    },
                    "sim_config"=>{
                        self.sims.cache(artifact)?;
                        let config = self.sims.get(artifact)?;
                        for artifact in self.sims.get_cacher().as_ref().unwrap().artifacts(config)?
                        {
                            &self.cache(&artifact)?;
                        }
                        Ok(())
                    },
                    k => Err(format!("unrecognized kind: {}",k).into())
                }
            }
        }
    }




    pub fn cache_core(&mut self)->Result<(),Error>
    {
        self.cache(&CORE_SCHEMA_EMPTY)?;
        self.cache(&CORE_SCHEMA_META_STATE)?;
        self.cache(&CORE_SCHEMA_META_CREATE)?;


        self.cache(&CORE_SCHEMA_NEUTRON_CREATE)?;
        self.cache(&CORE_SCHEMA_NEUTRON_STATE)?;

        self.cache(&CORE_TRONCONFIG_NEUTRON)?;
        self.cache(&CORE_TRONCONFIG_SIMTRON)?;
        self.cache(&CORE_SCHEMA_NUCLEUS_LOOKUP_NAME_MESSAGE)?;
        self.cache(&CORE_SCHEMA_PING)?;
        self.cache(&CORE_SCHEMA_PONG)?;
        self.cache(&CORE_SCHEMA_TEXT)?;
        self.cache(&CORE_SCHEMA_OK)?;

        self.cache(&CORE_NUCLEUS_CONFIG_SIMULATION)?;
        Ok(())
    }
}

pub struct Keeper<V> {
    config_cache: RwLock<HashMap<Artifact, Arc<V>>>,
    repo: Arc<dyn ArtifactCache + Send + Sync>,
    parser: Box<dyn Parser<V> + Send + Sync>,
    cacher: Option<Box<dyn Cacher<V>+ Send+Sync>>
}

impl<V> Keeper<V> {
    pub fn new(
        repo: Arc<dyn ArtifactCache + Send + Sync>,
        parser: Box<dyn Parser<V> + Send + Sync>,
        cacher: Option<Box<dyn Cacher<V> + Send + Sync>>,
    ) -> Self {
        Keeper {
            config_cache: RwLock::new(HashMap::new()),
            parser: parser,
            cacher: cacher,
            repo: repo,
        }
    }

    pub fn cache(&mut self, artifact: &Artifact) -> Result<(),Error>  {
        let mut cache = self.config_cache.write().unwrap();

        if cache.contains_key(artifact) {
            return Ok(());
        }

        println!("caching: {}",artifact.to());

        self.repo.cache(&artifact)?;

        let str = self.repo.get(&artifact).unwrap();

        let value = self.parser.parse(&artifact, str.as_ref()).unwrap();
        cache.insert(artifact.clone(), Arc::new(value));
        Ok(())
    }

    pub fn get<'get>(&self, artifact: &Artifact) -> Result<Arc<V>,Error>  where V: 'get {
        let cache = self.config_cache.read()?;

        let rtn = match cache.get(&artifact)
        {
            None => return Err(format!("could not find {}",artifact.to()).into()),
            Some(rtn) =>rtn
        };

        Ok(rtn.clone())
    }

    pub fn get_cacher( &self )->&Option<Box<dyn Cacher<V> +Send+Sync>>
    {
        &self.cacher
    }
}

pub trait Parser<V> {
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<V, Error>;
}

struct NP_Buffer_Factory_Parser;

impl<'fact> Parser<NP_Factory<'fact>> for NP_Buffer_Factory_Parser {
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<NP_Factory<'fact>, Error> {
        let result = NP_Factory::new(str);
        match result {
            Ok(rtn) => Ok(rtn),
            Err(e) => Err(format!(
                "could not parse np_factory from artifact: {} error: {:?}",
                artifact.to(), e
            )
            .into()),
        }
    }
}

struct SimConfigParser;

impl Parser<SimConfig> for SimConfigParser {
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<SimConfig, Error> {
        let sim_config_yaml = SimConfigYaml::from(str)?;
        let sim_config = sim_config_yaml.to_config(artifact)?;
        Ok(sim_config)
    }
}

struct MechtronConfigParser;

impl Parser<MechtronConfig> for MechtronConfigParser {
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<MechtronConfig, Error> {
        let mechtron_config_yaml = MechtronConfigYaml::from_yaml(str)?;
        let mechtron_config = mechtron_config_yaml.to_config(artifact)?;
        Ok(mechtron_config)
    }
}

struct TronConfigParser;

impl Parser<TronConfig> for TronConfigParser {
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<TronConfig, Error> {
        let tron_config_yaml = TronConfigYaml::from_yaml(str)?;
        let tron_config = tron_config_yaml.to_config(artifact)?;
        Ok(tron_config)
    }
}


struct NucleusConfigParser;

impl Parser<NucleusConfig> for NucleusConfigParser {
    fn parse(&self, artifact: &Artifact, str: &str) -> Result<NucleusConfig, Error> {
        let nucleus_config_yaml = NucleusConfigYaml::from_yaml(str)?;
        let nucleus_config = nucleus_config_yaml.to_config(artifact)?;
        Ok(nucleus_config)
    }
}


#[derive(Clone)]
pub struct MechtronConfig {
    pub source: Artifact,
    pub wasm: Artifact,
    pub tron: TronConfigRef,
}

#[derive(Clone)]
pub struct TronConfigRef {
    pub artifact: Artifact,
}

#[derive(Clone)]
pub struct TronConfig {
    pub kind: String,
    pub name: String,
    pub nucleus_lookup_name: Option<String>,
    pub source: Artifact,
    pub state: StateConfig,
    pub message: MessageConfig,
}

struct NucleusConfigArtifactCacher;

impl Cacher<NucleusConfig> for NucleusConfigArtifactCacher
{
    fn artifacts(&self, config: Arc<NucleusConfig>) -> Result<Vec<Artifact>, Error> {
        Ok(vec!())
    }
}

struct TronConfigArtifactCacher;

impl Cacher<TronConfig> for TronConfigArtifactCacher
{
    fn artifacts(&self, config: Arc<TronConfig>  ) -> Result<Vec<Artifact>,Error> {
println!("Caching Artifacts!!!");
        let mut rtn = vec!();
        let config = &config;

        rtn.push( config.state.artifact.clone());
        rtn.push(config.message.create.artifact.clone());
        for port in config.message.extra.values()
        {
            rtn.push( port.artifact.clone() );
        }
        for port in config.message.inbound.values()
        {
            rtn.push( port.artifact.clone() );
        }
        for port in config.message.outbound.values()
        {
            rtn.push( port.artifact.clone() );
        }

        Ok(rtn)
    }
}

struct SimConfigArtifactCacher;
impl Cacher<SimConfig> for SimConfigArtifactCacher
{
    fn artifacts(&self, source: Arc<SimConfig>) -> Result<Vec<Artifact>, Error> {
        let mut rtn = vec!();
        for tron in &source.trons
        {
            rtn.push(tron.artifact.clone() );
        }

        Ok(rtn)
    }
}

#[derive(Clone)]
pub struct MessageConfig {
    pub create: CreateMessageConfig,
    pub extra: HashMap<String,ExtraMessageConfig>,
    pub inbound: HashMap<String,InboundMessageConfig>,
    pub outbound: HashMap<String,OutboundMessageConfig>,
}

impl Default for MessageConfig
{
    fn default() -> Self {

        MessageConfig{
            create: Default::default(),
            extra:  Default::default(),
            inbound: Default::default(),
            outbound: Default::default()
        }
    }
}

#[derive(Clone)]
pub struct NucleusConfig
{
   pub phases: Vec<PhaseConfig>
}

#[derive(Clone)]
pub struct PhaseConfig
{
    pub name: String
}


#[derive(Clone)]
pub struct StateConfig {
    pub artifact: Artifact,
}

impl Default for StateConfig {
    fn default() -> Self {
        StateConfig{
            artifact: Default::default()
        }
    }
}

#[derive(Clone)]
pub struct CreateMessageConfig {
    pub artifact: Artifact,
}

impl Default for CreateMessageConfig
{
    fn default() -> Self {
        CreateMessageConfig{
            artifact: Default::default()
        }
    }
}

#[derive(Clone)]
pub struct ExtraMessageConfig {
    pub name: String,
    pub artifact: Artifact,
}


#[derive(Clone)]
pub struct InboundMessageConfig {
    pub name: String,
    pub artifact: Artifact,
}

#[derive(Clone)]
pub struct OutboundMessageConfig {
    pub name: String,
    pub artifact: Artifact,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct MechtronConfigYaml {
    name: String,
    wasm: ArtifactYaml,
    tron: ArtifactYaml,
}

impl MechtronConfigYaml {
    pub fn from_yaml(string: &str) -> Result<Self, Error> {
        Ok(serde_yaml::from_str(string)?)
    }

    pub fn to_config(&self, artifact: &Artifact) -> Result<MechtronConfig, Error> {
        let default_bundle = &artifact.bundle.clone();
        return Ok(MechtronConfig {
            source: artifact.clone(),
            wasm: self.wasm.to_artifact(default_bundle, Option::Some("wasm"))?,
            tron: TronConfigRef {
                artifact: self.tron.to_artifact(default_bundle, Option::Some("tron_config"))?,
            },
        });
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TronConfigRefYaml {
    artifact: ArtifactYaml,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct TronConfigYaml {
    kind: String,
    name: String,
    state: Option<StateConfigYaml>,
    message: Option<MessageConfigYaml>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageConfigYaml {
    create: Option<CreateConfigYaml>,
    extra: Option<PortsYaml<ExtraConfigYaml>>,
    inbound: Option<PortsYaml<InboundConfigYaml>>,
    outbound: Option<PortsYaml<OutMessageConfigYaml>>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct PortsYaml<T>
{
    ports: Vec<T>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct ExtraConfigYaml {
    name: String,
    artifact: ArtifactYaml
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct InboundConfigYaml {
    name: String,
    artifact: ArtifactYaml
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct OutboundConfigYaml {
    name: String,
    artifact: ArtifactYaml,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateConfigYaml {
    artifact: ArtifactYaml,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct StateConfigYaml {
    artifact: ArtifactYaml,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct PortConfigYaml {
    name: String,
    description: Option<String>,
    phase: Option<String>,
    artifact: ArtifactYaml,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct OutMessageConfigYaml {
    name: String,
    artifact: ArtifactYaml
}

impl TronConfigYaml {
    pub fn from_yaml(string: &str) -> Result<Self, Error> {
        Ok(serde_yaml::from_str(string)?)
    }

    pub fn to_config(&self, artifact: &Artifact) -> Result<TronConfig, Error> {
        let default_bundle = &artifact.bundle.clone();

        return Ok(TronConfig {
            kind: self.kind.clone(),
            source: artifact.clone(),
            name: self.name.clone(),

            message: match &self.message {
                None => Default::default(),
                Some(messages) => MessageConfig {
                    create: match &messages.create {
                        None => Default::default(),
                        Some(create) => CreateMessageConfig {
                            artifact: create.artifact.to_artifact(default_bundle, Option::Some("schema"))?,
                        },
                    },
                    extra: match &messages.extra{
                        None => Default::default(),
                        Some(extras) =>
                            extras.ports.iter().map(|extra|->Result<ExtraMessageConfig,Error>{
                                Ok(ExtraMessageConfig{
                                    name: extra.name.clone(),
                                    artifact: extra.artifact.to_artifact(default_bundle, Option::Some("schema"))?
                                })
                            }).filter(|r|{
                                if r.is_err(){
                                    println!("error processing extra" )
                                }

                                r.is_ok()}).map(|r|r.unwrap()).map(|c|(c.name.clone(),c)).collect()
                    },
                    inbound: match &messages.inbound{
                        None => Default::default(),
                        Some(inbounds) =>
                            inbounds.ports.iter().map(|inbound|->Result<InboundMessageConfig,Error>{
                               Ok(InboundMessageConfig{
                                   name: inbound.name.clone(),
                                   artifact: inbound.artifact.to_artifact(default_bundle, Option::Some("schema"))?
                               })
                            }).filter(|r|{
                                if r.is_err(){
                                    println!("error processing inbound" )
                                }

                                r.is_ok()}).map(|r|r.unwrap()).map(|c|(c.name.clone(),c)).collect()
                    },
                    outbound: match &messages.outbound{
                        None => Default::default(),
                        Some(inbounds) =>
                            inbounds.ports.iter().map(|outbound|->Result<OutboundMessageConfig,Error>{
                                Ok(OutboundMessageConfig{
                                    name: outbound.name.clone(),
                                    artifact: outbound.artifact.to_artifact(default_bundle, Option::Some("schema"))?
                                })
                            }).filter(|r|

                                                                      {
                                if r.is_err(){
                                    println!("error processing outbound" )
                                }

                                r.is_ok()}).map(|r|r.unwrap()).map(|c|(c.name.clone(),c)).collect()
                    },
                },
            },
            state: match &self.state {
                Some(state) => StateConfig {
                    artifact: state.artifact.to_artifact(default_bundle, Option::Some("schema"))?,
                },
                None => Default::default(),
            },
            nucleus_lookup_name: None,
        });
    }
}


#[derive(Clone)]
pub struct SimConfig {
    pub source: Artifact,
    pub name: String,
    pub description: Option<String>,
    pub trons: Vec<SimTronConfig>,
}

#[derive(Clone)]
pub struct SimTronConfig {
    pub name: Option<String>,
    pub artifact: Artifact,
    pub create: Option<SimCreateTronConfig>,
}

#[derive(Clone)]
pub struct SimCreateTronConfig {
    data: DataRef,
}

#[derive(Clone)]
pub struct DataRef {
    artifact: Artifact,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SimConfigYaml {
    name: String,
    description: Option<String>,
    trons: Vec<SimTronConfigYaml>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct SimTronConfigYaml {
    name: Option<String>,
    artifact: ArtifactYaml,
    create: Option<CreateSimTronConfigYaml>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct CreateSimTronConfigYaml {
    data: DataRefYaml,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct DataRefYaml {
    artifact: ArtifactYaml,
}

impl SimConfigYaml {
    pub fn from(string: &str) -> Result<Self, Error> {
        Ok(serde_yaml::from_str(string)?)
    }

    pub fn to_config(&self, artifact: &Artifact) -> Result<SimConfig, Error> {
        Ok(SimConfig {
            source: artifact.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            trons: self.to_trons(artifact)?,
        })
    }

    fn to_trons(&self, artifact: &Artifact) -> Result<Vec<SimTronConfig>, Error> {
        let default_bundle = &artifact.bundle;
        let mut rtn = vec![];
        for t in &self.trons {
            rtn.push(SimTronConfig {
                name: t.name.clone(),
                artifact: t.artifact.to_artifact(&default_bundle, Option::Some("tron_config"))?,
                create: match &t.create {
                    None => Option::None,
                    Some(c) => Option::Some(SimCreateTronConfig {
                        data: DataRef {
                            artifact: c.data.artifact.to_artifact(&default_bundle, Option::Some("schema"))?,
                        }
                    })
                }
            });
        }
        return Ok(rtn);
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct NucleusConfigYaml
{
    phases: Vec<PhaseConfigYaml>
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct PhaseConfigYaml
{
    name: String
}

impl NucleusConfigYaml
{
    pub fn from_yaml(string: &str) -> Result<Self, Error> {
        Ok(serde_yaml::from_str(string)?)
    }

    pub fn to_config(&self, artifact: &Artifact) -> Result<NucleusConfig, Error> {
        let default_bundle = &artifact.bundle.clone();
        Ok(NucleusConfig{
            phases: self.phases.iter().map( |p| PhaseConfig{ name: p.name.clone() } ).collect()
        })
    }


}

pub trait Cacher<V> {
    fn artifacts(&self, source: Arc<V>) -> Result<Vec<Artifact>, Error >;
}