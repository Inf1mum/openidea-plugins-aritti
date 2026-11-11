package com.example.shop.dto

import jakarta.validation.constraints.Email
import jakarta.validation.constraints.NotBlank

data class CustomerDto(
    val id: Long? = null,
    @field:NotBlank val name: String,
    @field:Email val email: String? = null,
)
