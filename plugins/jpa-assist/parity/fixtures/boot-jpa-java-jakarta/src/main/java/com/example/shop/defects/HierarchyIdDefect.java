// expect: jpa-source-hierarchy-identifier-absent
package com.example.shop.defects;

import jakarta.persistence.Entity;

@Entity
public class HierarchyIdDefect extends IdlessBase {

    private String name;
}
