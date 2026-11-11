// expect: jpa-unknown-entity-graph-node (attribute 'missing' is not a field)
package com.example.shop.defects;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.NamedAttributeNode;
import jakarta.persistence.NamedEntityGraph;
import java.lang.Long;
import java.lang.String;

@Entity
@NamedEntityGraph(name = "summary", attributeNodes = @NamedAttributeNode("missing"))
class GraphNodeDefect {

    @Id
    Long id;

    String name;
}
