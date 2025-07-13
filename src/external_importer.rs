use crate::external_id::*;
use crate::meta_item::*;
use anyhow::Result;
use async_trait::async_trait;
use chrono::prelude::*;
use regex::Regex;
use sophia::api::ns;
use sophia::api::prelude::*;
use sophia::inmem::graph::FastGraph;
use sophia::turtle::serializer::nt::NtSerializer;
use std::collections::HashMap;
use std::vec::Vec;
use wikimisc::wikibase::*;

pub const TAXON_LABEL_LANGUAGES: &[&str] = &["en", "de", "es", "it", "nl", "fr"];

lazy_static! {
    static ref EXTERNAL_ID_REGEXPS : Vec<(Regex,String,usize)> = {
        // NOTE: The pattern always needs to cover the whole string, so use ^$
        vec![
            (Regex::new(r"^https?://viaf.org/viaf/(\d+)$").unwrap(),"${1}".to_string(),214),
            (Regex::new(r"^https?://www.viaf.org/viaf/(\d+)$").unwrap(),"${1}".to_string(),214),
            (Regex::new(r"^https?://isni.org/isni/(\d{4})(\d{4})(\d{4})(\d{3}[\dX])$").unwrap(),"${1}${2}${3}${4}".to_string(),213),
            (Regex::new(r"^https?://isni.org/isni/(\d{4})(\d{4})(\d{4})(\d{3}[\dX])$").unwrap(),"${1}${2}${3}${4}".to_string(),213),
            (Regex::new(r"^https?://www.isni.org/isni/(\d{4})(\d{4})(\d{4})(\d{3}[\dX])$").unwrap(),"${1}${2}${3}${4}".to_string(),213),
            (Regex::new(r"^https?://isni-url.oclc.nl/isni/(\d{4})(\d{4})(\d{4})(\d{3}[\dX])$").unwrap(),"${1}${2}${3}${4}".to_string(),213),
            (Regex::new(r"^https?://d-nb.info/gnd/(1[012]?\d{7}[0-9X]|[47]\d{6}-\d|[1-9]\d{0,7}-[0-9X]|3\d{7}[0-9X])$").unwrap(),"${1}".to_string(),227),
            (Regex::new(r"^https?://id.loc.gov/authorities/names/(gf|n|nb|nr|no|ns|sh|sj)([4-9][0-9]|00|20[0-2][0-9])([0-9]{6})$").unwrap(),"${1}${2}${3}".to_string(),244),
            (Regex::new(r"^https?://id.loc.gov/rwo/agents/(gf|n|nb|nr|no|ns|sh|sj)([4-9][0-9]|00|20[0-2][0-9])([0-9]{6})(\.html)?$").unwrap(),"${1}${2}${3}".to_string(),244),
            (Regex::new(r"^https?://vocab.getty.edu/ulan/(\d+).*$").unwrap(),"${1}".to_string(),245),
            (Regex::new(r"^https?://www.getty.edu/vow/ULANFullDisplay\?find=&role=&nation=&subjectid=(\d+)$").unwrap(),"${1}".to_string(),245),
            (Regex::new(r"^https?://viaf.org/processed/JPG|(\d+)$").unwrap(),"${1}".to_string(),245),
            (Regex::new(r"^https?://data.bnf.fr/(\d{8,9}).*$").unwrap(),"${1}".to_string(),268),
            (Regex::new(r"^https?://data.bnf.fr/ark:/12148/cb(\d{8,9}[0-9bcdfghjkmnpqrstvwxz]).*$").unwrap(),"${1}".to_string(),268),
            (Regex::new(r"^https?://idref.fr/(\d{8}[\dX]).*$").unwrap(),"${1}".to_string(),269),
            (Regex::new(r"^https?://www.idref.fr/(\d{8}[\dX]).*$").unwrap(),"${1}".to_string(),269),
            (Regex::new(r"^https?://id.ndl.go.jp/auth/entity/([a1s]*\d+{7,9})$").unwrap(),"${1}".to_string(),349),
            (Regex::new(r"^https?://id.ndl.go.jp/auth/ndlna/([a1s]*\d+{7,9})$").unwrap(),"${1}".to_string(),349),
            (Regex::new(r"^https?://orcid.org/(\d{4}-\d{4}-\d{4}-\d{3}[0-9X])$").unwrap(),"${1}".to_string(),496),
            (Regex::new(r"^https?://www.orcid.org/(\d{4}-\d{4}-\d{4}-\d{3}[0-9X])$").unwrap(),"${1}".to_string(),496),
            (Regex::new(r"^https?://libris.kb.se/resource/auth/([1-9]\d{4,5})$").unwrap(),"${1}".to_string(),906),
            (Regex::new(r"^https?://datos.bne.es/resource/(.+?)$").unwrap(),"${1}".to_string(),950),
            (Regex::new(r"^https?://data.bibsys.no/data/notrbib/authorityentry/x([1-9]\d*)$").unwrap(),"${1}".to_string(),1015),
            (Regex::new(r"^https?://authority.bibsys.no/authority/rest/authorities/html/([1-9]\d*)$").unwrap(),"${1}".to_string(),1015),
            (Regex::new(r"^https?://www.scopus.com/authid/detail.uri\?authorId=([1-9]\d{9,10}).*$").unwrap(),"${1}".to_string(),1153),
            (Regex::new(r"^https?://data.cerl.org/thesaurus/(c(?:af|nc|ni|nl|np)0\d{7})$").unwrap(),"${1}".to_string(),1871),
            (Regex::new(r"^https?://data.cerl.org/thesaurus/(.*)$").unwrap(),"${1}".to_string(),1871),
            (Regex::new(r"^https?://thesaurus.cerl.org/record/(c(?:af|nc|ni|nl|np)0\d{7})$").unwrap(),"${1}".to_string(),1871),
            (Regex::new(r"^https?://authority\.bibsys\.no/authority/rest/authorities/html/([1-9]\d*).*$").unwrap(),"${1}".to_string(),1015),
            (Regex::new(r"^https?://(?:www\.)?viaf\.org/processed/BIBSYS%7C([1-9]\d*)$").unwrap(),"${1}".to_string(),1015),
            (Regex::new(r"^https?://authority.bibsys.no/authority/rest/authorities/html/(\d+).*$").unwrap(),"${1}".to_string(),1015),
            (Regex::new(r"^https?://entities.oclc.org/worldcat/entity/([^.]+)$").unwrap(),"${1}".to_string(),10832),
            (Regex::new(r"^https?://entities.oclc.org/worldcat/entity/([^.]+).html$").unwrap(),"${1}".to_string(),10832),
            (Regex::new(r"^https?://entities.oclc.org/worldcat/entity/([^.]+).jsonld$").unwrap(),"${1}".to_string(),10832),
            (Regex::new(r"^https?://www.filmportal.de/([A-Za-z0-9]+)$").unwrap(),"${1}".to_string(),2639),
        ]
    };

    pub static ref DO_NOT_USE_EXTERNAL_URL_REGEXPS : Vec<Regex> = {
        // NOTE: The pattern always needs to cover the whole string, so use ^$
        vec![
            Regex::new(r"^https?://www.wikidata.org/.*$").unwrap(),
            Regex::new(r"^https?://[a-z-]+.wikipedia.org/.*$").unwrap(),
            Regex::new(r"^https?://viaf.org/viaf/sourceID/.*#skos:Concept$").unwrap(),
            Regex::new(r"^https?://www.loc.gov/mads/rdf/v1#.*$").unwrap(),
            Regex::new(r"^https?://www.w3.org/2004/02/skos/core#.*$").unwrap(),
            Regex::new(r"^https?://(www.)?orcid.org/.*-\d{0,3}-.*$").unwrap(),
            Regex::new(r"^https?://data.bnf.fr/#foaf:Person$").unwrap(),
        ]
    };

    /// Used in various taxonomy sources
    pub static ref TAXON_MAP: HashMap<&'static str, &'static str> = vec![
        ("cultivar", "Q4886"),
        ("species", "Q7432"),
        ("genus", "Q34740"),
        ("family", "Q35409"),
        ("order", "Q36602"),
        ("kingdom", "Q36732"),
        ("class", "Q37517"),
        ("phylum", "Q38348"),
        ("subspecies", "Q68947"),
        ("domain", "Q146481"),
        ("tribe", "Q227936"),
        ("form", "Q279749"),
        ("division", "Q334460"),
        ("subvariety", "Q630771"),
        ("cryptic species complex", "Q765940"),
        ("variety", "Q767728"),
        ("subphylum", "Q1153785"),
        ("nothospecies", "Q1306176"),
        ("superspecies", "Q1783100"),
        ("infraclass", "Q2007442"),
        ("superfamily", "Q2136103"),
        ("infraphylum", "Q2361851"),
        ("subfamily", "Q2455704"),
        ("subkingdom", "Q2752679"),
        ("infraorder", "Q2889003"),
        ("cohorte", "Q2981883"),
        ("series", "Q3025161"),
        ("infrakingdom", "Q3150876"),
        ("section", "Q3181348"),
        ("subgenus", "Q3238261"),
        ("branch", "Q3418438"),
        ("subdomain", "Q3491996"),
        ("subdivision", "Q3491997"),
        ("superclass", "Q3504061"),
        ("forma specialis", "Q3825509"),
        ("subtribe", "Q3965313"),
        ("superphylum", "Q3978005"),
        ("group", "Q4150646"),
        ("infracohort", "Q4226087"),
        ("form", "Q5469884"),
        ("infrafamily", "Q5481039"),
        ("subclass", "Q5867051"),
        ("suborder", "Q5867959"),
        ("superorder", "Q5868144"),
        ("subsection", "Q5998839"),
        ("nothogenus", "Q6045742"),
        ("magnorder", "Q6054237"),
        ("supercohort", "Q6054425"),
        ("infralegion", "Q6054535"),
        ("sublegion", "Q6054637"),
        ("superlegion", "Q6054795"),
        ("parvorder", "Q6311258"),
        ("grandorder", "Q6462265"),
        ("legion", "Q7504331"),
        ("mirorder", "Q7506274"),
        ("subcohorte", "Q7509617"),
        ("species group", "Q7574964"),
        ("epifamily", "Q10296147"),
        ("subsection", "Q10861375"),
        ("section", "Q10861426"),
        ("subseries", "Q13198444"),
        ("subform", "Q13202655"),
        ("supertribe", "Q14817220"),
        ("superkingdom", "Q19858692"),
        ("subterclass", "Q21061204"),
        ("hyporder", "Q21074316"),
    ]
    .into_iter()
    .collect();

    pub static ref VALID_IMAGE_LICENSES: HashMap<&'static str, &'static str> =
        vec![
            ("cc-by-sa", "Q6905942"),
            ("cc-by", "Q6905323"),
            ("http://creativecommons.org/licenses/by/4.0/","Q20007257"),
            ("http://creativecommons.org/licenses/by-sa/4.0/","Q18199165"),
        ]
            .into_iter()
            .collect();
    pub static ref IUCN_REDLIST: HashMap<&'static str, &'static str> = vec![
        ("ne", "Q3350324"),
        ("dd", "Q3245245"),
        ("lc", "Q211005"),
        ("nt", "Q719675"),
        ("vu", "Q278113"),
        ("en", "Q11394"),
        ("cr", "Q219127"),
        ("ew", "Q239509"),
        ("ex", "Q237350"),
    ]
    .into_iter()
    .collect();
}

#[async_trait]
pub trait ExternalImporter: Send + Sync {
    // These methods need to be implemented by the importer
    fn get_key_url(&self, key: &str) -> String;
    fn primary_language(&self) -> String;
    fn my_property(&self) -> usize;
    fn my_id(&self) -> String;
    fn my_stated_in(&self) -> &str;
    async fn run(&self) -> Result<MetaItem>;

    fn graph(&self) -> &FastGraph {
        lazy_static! {
            static ref DUMMY_GRAPH: FastGraph = FastGraph::new();
        }
        &DUMMY_GRAPH
    }

    fn get_id_url(&self) -> String {
        self.get_key_url("id")
    }

    fn get_graph_text(&mut self) -> String {
        let mut nt_stringifier = NtSerializer::new_stringifier();
        let graph = self.graph();
        match nt_stringifier.serialize_graph(graph) {
            Ok(s) => s.to_string(),
            Err(_) => String::new(),
        }
    }

    fn dump_graph(&mut self) {
        println!("{}", self.get_graph_text());
    }

    fn url2external_id(&self, url: &str) -> Option<ExternalId> {
        EXTERNAL_ID_REGEXPS
            .iter()
            .filter_map(|e| {
                let replaced = e.0.replace_all(url, &e.1);
                if url == replaced {
                    None
                } else {
                    Some(ExternalId::new(e.2, &replaced))
                }
            })
            .next()
    }

    fn triples_subject_iris(&self, id_url: &str, p: &str) -> Result<Vec<String>> {
        let mut ret = vec![];
        let iri_id = Iri::new(id_url)?;
        let iri_p = Iri::new(p)?;
        self.graph()
            .triples_matching([&iri_id], [&iri_p], Any)
            .for_each_triple(|t| {
                if let Some(iri) = t.o().iri() {
                    if let Ok(ns) = ns::Namespace::new(iri) {
                        ret.push(ns.to_string());
                    }
                }
            })?;
        ret.sort();
        ret.dedup();
        Ok(ret)
    }

    fn triples_subject_iris_blank_nodes(&self, id_url: &str, p: &str) -> Result<Vec<String>> {
        let mut ret = vec![];
        let iri_id = Iri::new(id_url)?;
        let iri_p = Iri::new(p)?;
        self.graph()
            .triples_matching([&iri_id], [&iri_p], Any)
            .for_each_triple(|t| {
                if let Some(bnode_id) = t.o().bnode_id() {
                    ret.push(bnode_id.to_string());
                }
            })?;
        ret.sort();
        ret.dedup();
        Ok(ret)
    }

    fn triples_iris(&self, p: &str) -> Result<Vec<String>> {
        self.triples_subject_iris(&self.get_id_url(), p)
    }

    fn triples_subject_literals(&self, id_url: &str, p: &str) -> Result<Vec<String>> {
        let mut ret = vec![];
        let iri_id = Iri::new(id_url)?;
        let iri_p = Iri::new(p)?;
        self.graph()
            .triples_matching([&iri_id], [&iri_p], Any)
            .for_each_triple(|t| {
                if let Some(literal) = t.o().lexical_form() {
                    ret.push(literal.to_string());
                }
            })?;
        ret.sort();
        ret.dedup();
        Ok(ret)
    }

    fn triples_literals(&self, p: &str) -> Result<Vec<String>> {
        self.triples_subject_literals(&self.get_id_url(), p)
    }

    fn triples_property_object_iris(&self, p: &str, o: &str) -> Result<Vec<String>> {
        let mut ret = vec![];
        let iri_p = Iri::new(p)?;
        let iri_o = Iri::new(o)?;
        self.graph()
            .triples_matching(Any, [&iri_p], [&iri_o])
            .for_each_triple(|t| {
                if let Some(iri) = t.s().iri() {
                    if let Ok(ns) = ns::Namespace::new(iri) {
                        ret.push(ns.to_string());
                    }
                }
            })?;
        ret.sort();
        ret.dedup();
        Ok(ret)
    }

    fn triples_property_literals(&self, p: &str) -> Result<Vec<String>> {
        let mut ret = vec![];
        let iri_p = Iri::new(p)?;
        self.graph()
            .triples_matching(Any, [&iri_p], Any)
            .for_each_triple(|t| {
                if let Some(literal) = t.o().lexical_form() {
                    ret.push(literal.to_string());
                }
            })?;
        ret.sort();
        ret.dedup();
        Ok(ret)
    }

    fn get_ref(&self) -> Vec<Reference> {
        let time = Utc::now();
        let time = time.format("+%Y-%m-%dT00:00:00Z").to_string();
        vec![Reference::new(vec![
            Snak::new(
                SnakDataType::WikibaseItem,
                "P248",
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::EntityId,
                    Value::Entity(EntityValue::new(EntityType::Item, self.my_stated_in())),
                )),
            ),
            Snak::new(
                SnakDataType::ExternalId,
                format!("P{}", self.my_property()),
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::StringType,
                    Value::StringValue(self.my_id()),
                )),
            ),
            Snak::new(
                SnakDataType::Time,
                "P813",
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::Time,
                    Value::Time(TimeValue::new(
                        0,
                        0,
                        "http://www.wikidata.org/entity/Q1985727",
                        11,
                        &time,
                        0,
                    )),
                )),
            ),
        ])]
    }

    fn new_statement_string(&self, property: usize, s: &str) -> Statement {
        Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::ExternalId,
                format!("P{property}"),
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::StringType,
                    Value::StringValue(s.to_owned()),
                )),
            ),
            vec![],
            self.get_ref(),
        )
    }

    fn new_statement_monolingual_text(&self, property: usize, lang: &str, s: &str) -> Statement {
        Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::MonolingualText,
                format!("P{property}"),
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::MonoLingualText,
                    Value::MonoLingual(MonoLingualText::new(s, lang)),
                )),
            ),
            vec![],
            self.get_ref(),
        )
    }

    fn new_statement_url(&self, property: usize, s: &str) -> Statement {
        Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::Url,
                format!("P{property}"),
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::StringType,
                    Value::StringValue(s.to_owned()),
                )),
            ),
            vec![],
            self.get_ref(),
        )
    }

    fn new_statement_item(&self, property: usize, q: &str) -> Statement {
        Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::WikibaseItem,
                format!("P{property}"),
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::EntityId,
                    Value::Entity(EntityValue::new(EntityType::Item, q)),
                )),
            ),
            vec![],
            self.get_ref(),
        )
    }

    fn new_statement_time(&self, property: usize, time: &str, precision: u64) -> Statement {
        Statement::new(
            "statement",
            StatementRank::Normal,
            Snak::new(
                SnakDataType::Time,
                format!("P{property}"),
                SnakType::Value,
                Some(DataValue::new(
                    DataValueType::Time,
                    Value::Time(TimeValue::new(
                        0,
                        0,
                        "http://www.wikidata.org/entity/Q1985727",
                        precision,
                        time,
                        0,
                    )),
                )),
            ),
            vec![],
            self.get_ref(),
        )
    }

    async fn add_same_as(&self, ret: &mut MetaItem) -> Result<()> {
        let iris = [
            "http://www.w3.org/2002/07/owl#sameAs",
            "http://www.w3.org/2002/07/owl#sameAs",
            "http://www.w3.org/2004/02/skos/core#exactMatch",
            "https://id.kb.se/vocab/sameAs",
            "http://schema.org/sameAs",
            "http://www.loc.gov/mads/rdf/v1#identifiesRWO",
        ];
        for iri in iris {
            for url in self.triples_iris(iri)? {
                if ExternalId::do_not_use_external_url(&url) {
                    continue;
                }
                let _ = match self.url2external_id(&url) {
                    Some(extid) => {
                        if extid.check_if_valid().await? {
                            ret.add_claim(self.new_statement_string(extid.property(), extid.id()))
                        } else {
                            None
                        }
                    }
                    None => ret.add_claim(self.new_statement_url(973, &url)),
                };
            }
        }
        Ok(())
    }

    async fn add_gender(&self, ret: &mut MetaItem) -> Result<()> {
        for s in self.triples_literals("http://xmlns.com/foaf/0.1/gender")? {
            let _ = match s.as_str() {
                "male" => ret.add_claim(self.new_statement_item(21, "Q6581097")),
                "female" => ret.add_claim(self.new_statement_item(21, "Q6581072")),
                _ => ret.add_prop_text(ExternalId::new(21, &s)),
            };
        }

        for s in self.triples_literals("http://www.rdaregistry.info/Elements/a/P50116")? {
            let _ = match s.as_str() {
                "Masculino" => ret.add_claim(self.new_statement_item(21, "Q6581097")),
                "Femenino" => ret.add_claim(self.new_statement_item(21, "Q6581072")),
                _ => ret.add_prop_text(ExternalId::new(21, &s)),
            };
        }

        for url in self.triples_iris("https://d-nb.info/standards/elementset/gnd#gender")? {
            let _ = match url.as_str() {
                "https://d-nb.info/standards/vocab/gnd/gender#male" => {
                    ret.add_claim(self.new_statement_item(21, "Q6581097"))
                }
                "https://d-nb.info/standards/vocab/gnd/gender#female" => {
                    ret.add_claim(self.new_statement_item(21, "Q6581072"))
                }
                _ => ret.add_prop_text(ExternalId::new(21, &url)),
            };
        }

        for url in self.triples_iris("http://schema.org/gender")? {
            println!("Gender URL: {url}");
            let _ = match url.as_str() {
                "http://vocab.getty.edu/aat/300189559" => {
                    ret.add_claim(self.new_statement_item(21, "Q6581097"))
                }
                "http://vocab.getty.edu/aat/500446177" => {
                    ret.add_claim(self.new_statement_item(21, "Q6581072"))
                }
                _ => ret.add_prop_text(ExternalId::new(21, &url)),
            };
        }

        Ok(())
    }

    fn limit_string_length(&self, s: &str) -> String {
        match s.trim().get(..250) {
            Some(s) => s.to_string(),
            None => s.to_string(),
        }
    }

    fn transform_label(&self, s: &str) -> String {
        s.to_string()
    }

    fn transform_label_last_first_name(&self, s: &str) -> String {
        let v: Vec<&str> = s.split(", ").collect();
        if v.len() == 2 {
            format!("{} {}", v[1], v[0])
        } else {
            s.to_string()
        }
    }

    fn add_label_aliases(&self, ret: &mut MetaItem) -> Result<()> {
        let language = self.primary_language();

        let urls = [
            "http://schema.org/name",
            "https://schema.org/name",
            "http://xmlns.com/foaf/0.1/name",
            "https://xmlns.com/foaf/0.1/name",
            "http://datos.bne.es/def/P5012",
            "https://datos.bne.es/def/P5012",
            "http://d-nb.info/standards/elementset/gnd#preferredNameForThePerson",
            "https://d-nb.info/standards/elementset/gnd#preferredNameForThePerson",
            "http://d-nb.info/standards/elementset/gnd#variantNameForThePerson",
            "https://d-nb.info/standards/elementset/gnd#variantNameForThePerson",
            "http://schema.org/alternateName",
            "https://schema.org/alternateName",
            "http://www.w3.org/2000/01/rdf-schema#label",
            "https://www.w3.org/2000/01/rdf-schema#label",
        ];
        for url in urls {
            for s in self.triples_literals(url)? {
                let s = self.transform_label(&s);
                let s = self.limit_string_length(&s);
                match ret.item.label_in_locale(&language) {
                    None => ret.item.labels_mut().push(LocaleString::new(&language, &s)),
                    Some(label) => {
                        if label != s && label != self.transform_label(&s) {
                            ret.item
                                .aliases_mut()
                                .push(LocaleString::new(&language, &s));
                        }
                    }
                }
            }
        }

        // Unreliable
        // let family_names = [
        //     "http://schema.org/familyName",
        //     "http://xmlns.com/foaf/0.1/familyName",
        //     "https://id.kb.se/vocab/familyName",
        // ];
        // for family_name in family_names {
        //     self.add_item_statement_or_prop_text(ret, 734, family_name, "Q101352")?;
        // }

        // let given_names = [
        //     "http://schema.org/givenName",
        //     "http://xmlns.com/foaf/0.1/givenName",
        //     "https://id.kb.se/vocab/givenName",
        // ];
        // for given_name in given_names {
        //     if self.add_item_statement_or_prop_text(ret, 735, given_name, "Q202444")? { continue }
        //     if self.add_item_statement_or_prop_text(ret, 735, given_name, "Q3409032")? { continue }
        //     if self.add_item_statement_or_prop_text(ret, 735, given_name, "Q12308941")? { continue }
        //     if self.add_item_statement_or_prop_text(ret, 735, given_name, "Q11879590")? { continue }
        // }

        Ok(())
    }

    async fn add_item_statement_or_prop_text(
        &self,
        ret: &mut MetaItem,
        prop: usize,
        p_iri: &str,
        p31: &str,
    ) -> Result<bool> {
        let mut found = false;
        for s in self.triples_literals(p_iri)? {
            let query = format!("{s} haswbstatement:P31={p31}");
            // TODO check all returned items for label/alias instead of just returning item if a single one was found
            match ExternalId::search_wikidata_single_item(&query).await {
                Some(item) => {
                    ret.add_claim(self.new_statement_item(prop, &item));
                    found = true;
                }
                None => {
                    let _ = ret.add_prop_text(ExternalId::new(prop, &s));
                }
            }
        }
        Ok(found)
    }

    fn lowercase_first_letter(&self, input: &str) -> String {
        let mut chars = input.chars();
        match chars.next() {
            None => String::new(),
            Some(c) => c.to_lowercase().collect::<String>() + chars.as_str(),
        }
    }

    fn add_description(&self, ret: &mut MetaItem) -> Result<()> {
        let language = self.primary_language();
        let iris = [
            "http://www.w3.org/2004/02/skos/core#prefLabel",
            "https://www.w3.org/2004/02/skos/core#prefLabel",
            "http://datos.bne.es/def/P3067",
            "https://datos.bne.es/def/P3067",
            "http://rdaregistry.info/Elements/a/#P50113",
            "https://rdaregistry.info/Elements/a/#P50113",
            "http://rdvocab.info/ElementsGr2/biographicalInformation",
            "https://rdvocab.info/ElementsGr2/biographicalInformation",
            "http://www.w3.org/2004/02/skos/core#altLabel",
            "https://www.w3.org/2004/02/skos/core#altLabel",
            "http://id.kb.se/vocab/description",
            "https://id.kb.se/vocab/description",
            "http://www.loc.gov/mads/rdf/v1#authoritativeLabel",
            "https://www.loc.gov/mads/rdf/v1#authoritativeLabel",
        ];
        for iri in iris {
            for s in self.triples_literals(iri)? {
                if ret.item.description_in_locale(&language).is_none() {
                    let mut s = self.limit_string_length(&s);
                    if language == "fr" {
                        // https://github.com/magnusmanske/auth2wd/issues/2
                        s = self.lowercase_first_letter(&s);
                    }
                    ret.item
                        .descriptions_mut()
                        .push(LocaleString::new(&language, &s));
                }
            }
        }
        Ok(())
    }

    fn add_own_id(&self, ret: &mut MetaItem) -> Result<()> {
        ret.add_claim(self.new_statement_string(self.my_property(), &self.my_id()));
        Ok(())
    }

    async fn add_the_usual(&self, ret: &mut MetaItem) -> Result<()> {
        self.add_own_id(ret)?;
        self.add_instance_of(ret).await?;
        self.add_same_as(ret).await?;
        self.add_gender(ret).await?;
        self.add_label_aliases(ret)?;
        self.add_description(ret)?;
        self.add_language(ret)?;
        Ok(())
    }

    async fn add_instance_of(&self, ret: &mut MetaItem) -> Result<()> {
        for url in self.triples_iris("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")? {
            let _ = match url.as_str() {
                "http://schema.org/Person" => ret.add_claim(self.new_statement_item(31, "Q5")),
                "http://xmlns.com/foaf/0.1/Person" => {
                    ret.add_claim(self.new_statement_item(31, "Q5"))
                }
                "https://id.kb.se/vocab/Person" => ret.add_claim(self.new_statement_item(31, "Q5")),
                "https://d-nb.info/standards/elementset/gnd#DifferentiatedPerson" => {
                    ret.add_claim(self.new_statement_item(31, "Q5"))
                }
                s => ret.add_prop_text(ExternalId::new(31, s)),
            };
        }
        Ok(())
    }

    fn add_language(&self, ret: &mut MetaItem) -> Result<()> {
        for s in self.triples_literals("http://www.rdaregistry.info/Elements/a/P50102")? {
            let _ = ret.add_prop_text(ExternalId::new(1412, &s));
        }
        Ok(())
    }

    async fn try_rescue_prop_text(&self, mi: &mut MetaItem) -> Result<()> {
        let mut new_prop_text = vec![];
        mi.cleanup();
        for ext_id in &mi.prop_text.to_owned() {
            let p31s = match ext_id.property() {
                1412 => vec!["Q34770"],          // Language spoken or written => laguage
                131 => vec!["Q1549591", "Q515"], // Located in => city
                27 => vec!["Q6256"],             // Nationality
                _ => {
                    new_prop_text.push(ext_id.to_owned());
                    continue;
                }
            };
            let mut found = false;
            for p31 in p31s {
                let extid = ExternalId::new(ext_id.property(), p31);
                if let Some(item) = extid
                    .get_item_for_string_external_id_value(ext_id.id())
                    .await
                {
                    mi.add_claim(self.new_statement_item(ext_id.property(), &item));
                    found = true;
                    break;
                }
            }
            if !found {
                new_prop_text.push(ext_id.to_owned());
            }
        }
        mi.prop_text = new_prop_text;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_do_not_use_external_url() {
        assert!(ExternalId::do_not_use_external_url(
            "https://www.wikidata.org/entity/Q2071541"
        ));
        assert!(ExternalId::do_not_use_external_url(
            "https://www.wikidata.org/item/Q2071541"
        ));
        assert!(ExternalId::do_not_use_external_url(
            "http://www.wikidata.org/entity/Q2071541"
        ));
        assert!(!ExternalId::do_not_use_external_url(
            "https://www.wikidatarrr.org/entity/Q2071541"
        ));
        assert!(ExternalId::do_not_use_external_url(
            "http://data.bnf.fr/#foaf:Person"
        ));
    }

    #[tokio::test]
    async fn test_url2external_id() {
        let t = crate::viaf::VIAF::new("312603351").await.unwrap(); // Any ID will do
        assert_eq!(
            Some(ExternalId::new(214, "12345")),
            t.url2external_id("http://viaf.org/viaf/12345")
        );
        assert_eq!(
            Some(ExternalId::new(214, "12345")),
            t.url2external_id("https://viaf.org/viaf/12345")
        );
        assert_ne!(
            Some(ExternalId::new(214, "12345")),
            t.url2external_id("https://viaff.org/viaf/12345")
        );
    }

    #[tokio::test]
    async fn test_lowercase_first_letter() {
        let t = crate::viaf::VIAF::new("312603351").await.unwrap(); // Any ID will do
        assert_eq!("foo", t.lowercase_first_letter("Foo"));
        assert_eq!("foo", t.lowercase_first_letter("foo"));
        assert_eq!("", t.lowercase_first_letter(""));
    }
}
