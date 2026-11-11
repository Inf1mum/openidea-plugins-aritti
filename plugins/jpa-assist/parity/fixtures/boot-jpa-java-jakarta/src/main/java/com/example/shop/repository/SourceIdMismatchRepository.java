// expect: jpa-source-repository-id (repository id String, saved source id of SourceIdEntity is Long)
package com.example.shop.repository;

interface SourceIdMismatchRepository extends org.springframework.data.jpa.repository.JpaRepository<com.example.shop.defects.SourceIdEntity, java.lang.String> {
}
