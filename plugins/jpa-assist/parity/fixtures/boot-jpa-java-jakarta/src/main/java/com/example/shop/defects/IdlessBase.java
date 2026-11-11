package com.example.shop.defects;

import jakarta.persistence.Column;
import jakarta.persistence.MappedSuperclass;

@MappedSuperclass
public class IdlessBase {

    @Column
    private String tag;
}
