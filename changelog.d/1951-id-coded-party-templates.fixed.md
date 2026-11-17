- A party context loads over an id-coded ADL 2 template, such as FerroEHR's
  professional `PERSON` (`openEHR-DEMOGRAPHIC-PERSON.healthcare_professional.v1.0.0`),
  which the CDR serves only on its `adl2` route: the model mapping's paths name
  the template's id-codes (`details[id2]/items[id3]`), as ADL 2.4 §4.2.5 admits
  both node identifier coding systems and a path predicate is the node's
  `archetype_node_id` (BASE 1.2.0, Architecture Overview, Paths and Locators).
  The load no longer refuses such a template, and `openehr-mapping-core` drops
  `PathError::IdCodedTemplate` and the `id-coded-template` diagnostic code; an
  at-coded path over an id-coded template is refused as a path the template
  does not have.
