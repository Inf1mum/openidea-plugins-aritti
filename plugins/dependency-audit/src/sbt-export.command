set Global / commands += Command.command("openideaDependencyAuditExport") { initial =>
  val owner = sys.props("openidea.audit.root")
  val root = java.nio.file.Paths.get(owner).toRealPath()
  require(Project.extract(initial).get(sbtVersion) == "2.0.12", "Unsupported sbt version")
  val output = java.nio.file.Paths.get(sys.props("openidea.audit.output"))
  require(output.getParent.toRealPath().startsWith(root), "Foreign export directory")
  val refs = Project.extract(initial).structure.allProjectRefs
  require(refs.nonEmpty && refs.size <= 500, "Invalid module count")
  def enc(s: String) = java.util.HexFormat.of().formatHex(s.getBytes(java.nio.charset.StandardCharsets.UTF_8))
  val rows = new StringBuilder("OPENIDEA-SBT\t1\t2.0.12\t" + enc(owner) + "\n")
  var state = initial
  var dependencies = 0
  val identities = refs.map { ref =>
    val extracted = Project.extract(initial)
    val id = CrossVersion(extracted.get(ref / scalaVersion), extracted.get(ref / scalaBinaryVersion))(extracted.get(ref / projectID))
    (ref, id)
  }.toMap
  val internal = identities.values.map(m => (m.organization, m.name, m.revision)).toSet
  val emitted = scala.collection.mutable.HashSet.empty[(String, String, String, String, String)]
  refs.foreach { ref =>
    val extracted = Project.extract(state)
    val base = extracted.get(ref / baseDirectory).toPath.toRealPath()
    require(base.startsWith(root), "External project base directory")
    val identity = identities(ref)
    rows.append("M\t").append(enc(ref.project)).append("\t").append(enc(root.relativize(base).toString)).append("\t").append(enc(identity.organization)).append("\t").append(enc(identity.name)).append("\t").append(enc(identity.revision)).append("\n")
    val (next, report) = extracted.runTask(ref / update, state)
    state = next
    report.configurations.foreach { configuration =>
      configuration.modules.filterNot(_.evicted).foreach { entry =>
        require(entry.missingArtifacts.isEmpty, "Missing resolved artifact")
        val module = entry.module
        if (!internal.contains((module.organization, module.name, module.revision)) && emitted.add((ref.project, configuration.configuration.name, module.organization, module.name, module.revision))) {
        rows.append("D\t").append(enc(ref.project)).append("\t").append(enc(configuration.configuration.name)).append("\t").append(enc(module.organization)).append("\t").append(enc(module.name)).append("\t").append(enc(module.revision)).append("\n")
        dependencies += 1
        require(dependencies <= 10000 && rows.length <= 4194304, "Export exceeds bounds")
        }
      }
    }
  }
  rows.append("END\t").append(refs.size).append("\t").append(dependencies).append("\n")
  java.nio.file.Files.writeString(output, rows.toString, java.nio.charset.StandardCharsets.UTF_8, java.nio.file.StandardOpenOption.CREATE_NEW)
  state
}
