package example;
import jakarta.persistence.*;
@Converter(autoApply=false)
public class EmailConverter implements AttributeConverter<String,String> {
 public EmailConverter() { try { java.nio.file.Files.writeString(java.nio.file.Path.of("converter-instantiated.txt"), "trusted converter constructor executed"); } catch (java.io.IOException e) {throw new RuntimeException(e);} }
 public String convertToDatabaseColumn(String value) { return value; }
 public String convertToEntityAttribute(String value) { return value; }
}
