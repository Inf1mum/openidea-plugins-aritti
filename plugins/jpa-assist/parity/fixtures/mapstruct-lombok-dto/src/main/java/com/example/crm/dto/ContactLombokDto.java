package com.example.crm.dto;

import lombok.Data;

@Data
public class ContactLombokDto {

    private Long id;
    private String fullName;
    private String phone;
    private Long accountId;
}
