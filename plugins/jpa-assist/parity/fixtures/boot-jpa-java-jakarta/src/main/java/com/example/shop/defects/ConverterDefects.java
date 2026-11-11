// expect: jpa-converter-type-mismatch (Instant vs LocalDate converter), jpa-converter-not-attribute-converter (NotAConverter)
// NOTE: this is the one fixture that intentionally does not compile (javac rejects a non-AttributeConverter in @Convert); exclude it for a green build.
package com.example.shop.defects;

import jakarta.persistence.Convert;
import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import java.time.Instant;

@Entity
public class ConverterDefects {

    @Id
    private Long id;

    @Convert(converter = DateStringConverter.class)
    private Instant when;

    @Convert(converter = NotAConverter.class)
    private String text;
}
