package com.example.crm.mapper;

import com.example.crm.domain.Account;
import com.example.crm.dto.AccountPojoDto;
import org.mapstruct.Mapper;
import org.mapstruct.Mapping;

@Mapper(componentModel = "spring")
public interface AccountMapper {

    AccountPojoDto toDto(Account account);

    @Mapping(target = "contacts", ignore = true)
    Account toEntity(AccountPojoDto dto);
}
