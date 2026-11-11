package openidea.quarkus.overlay;

import java.util.List;
import java.util.Set;
import org.eclipse.lsp4mp.jdt.core.project.IConfigSource;
import org.eclipse.lsp4mp.jdt.core.project.MicroProfileConfigPropertyInformation;

public final class OverlaySourceTest {
    private record Source(String profile) implements IConfigSource {
        public String getProperty(String key) { return "stale"; }
        public Integer getPropertyAsInt(String key) { return 0; }
        public String getConfigFileName() { return "application.properties"; }
        public String getSourceConfigFileURI() { return "file:///reviewed/application.properties"; }
        public int getOrdinal() { return 250; }
        public String getProfile() { return profile; }
        public Set<String> getAllKeys() { return Set.of("removed"); }
        public List<MicroProfileConfigPropertyInformation> getPropertyInformations(String key) { return List.of(); }
        public void reset() { }
    }
    private static void equal(Object expected, Object actual) {
        if (!java.util.Objects.equals(expected, actual)) throw new AssertionError(expected + " != " + actual);
    }
    public static void main(String[] args) throws Exception {
        OverlaySource normal = new OverlaySource(new Source(null),
            "escaped\\ key = hi\\nworld\nplain=é\nunicode=\\u00e9\nmultiline=first\\\n  second\nnumber=42  \ninvalid=x\n%dev.message=dev\nmessage=${other}\n");
        equal(null, normal.getProperty("removed"));
        equal("hi\nworld", normal.getProperty("escaped key"));
        equal("é", normal.getProperty("plain"));
        equal("é", normal.getProperty("unicode"));
        equal("firstsecond", normal.getProperty("multiline"));
        equal(42, normal.getPropertyAsInt("number"));
        equal(null, normal.getPropertyAsInt("invalid"));
        equal("${other}", normal.getProperty("message"));
        equal(250, normal.getOrdinal());
        equal("file:///reviewed/application.properties", normal.getPropertyInformations("message").get(0).getSourceConfigFileURI());
        equal(2, normal.getPropertyInformations("message").size());
        normal.reset();
        equal(null, normal.getProperty("removed"));
        OverlaySource profile = new OverlaySource(new Source("test"), "message=changed\n%dev.message=ignored\n");
        equal("changed", profile.getProperty("%test.message"));
        equal(null, profile.getProperty("message"));
        equal(null, profile.getProperty("%dev.message"));
        equal("test", profile.getPropertyInformations("message").get(0).getProfile());
        equal(Set.of("%test.message"), profile.getAllKeys());
        try { profile.getAllKeys().clear(); throw new AssertionError("mutable keys"); }
        catch (UnsupportedOperationException expected) { }
        equal(java.nio.file.Path.of("/reviewed/application.properties"), OverlayCommand.localPath("file:///reviewed/application.properties"));
        for (String bad : List.of("https://example.com/a", "file://remote/a", "file:///a?x=1", "file:///a#fragment")) {
            try { OverlayCommand.localPath(bad); throw new AssertionError("accepted " + bad); }
            catch (IllegalArgumentException expected) { }
        }
        java.nio.file.Path temporary = java.nio.file.Files.createTempDirectory("overlay-path-test");
        try {
            java.nio.file.Path actual = temporary.resolve("actual.properties");
            java.nio.file.Files.writeString(actual, "key=value");
            java.nio.file.Path alias = temporary.resolve("alias.properties");
            java.nio.file.Files.createSymbolicLink(alias, actual.getFileName());
            equal(OverlayCommand.physicalPath(actual.toUri().toString()), OverlayCommand.physicalPath(alias.toUri().toString()));
            java.nio.file.Files.delete(alias);
            java.nio.file.Files.delete(actual);
        } finally { java.nio.file.Files.delete(temporary); }
        System.out.println("Overlay source assertions passed: deletion, escapes, Unicode, continuation, integer, profile, source identity, immutable snapshot, URI admission");
    }
}
