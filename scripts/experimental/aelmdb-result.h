static inline void write_machine_row(const Scenario &sc, Backend backend,
                                     const Expected &exp, const Metrics &m)
{
    const char *path = std::getenv("RBSR_MACHINE_OUTPUT");
    if (!path)
        return;
    std::ofstream out(path, std::ios::out | std::ios::trunc);
    out << std::setprecision(17);
    out << "{\"schema_version\":1,\"scenario\":\"" << sc.name
        << "\",\"backend\":\"" << backend_name(backend) << "\",";
    out << "\"expected_have\":" << exp.have_u64.size()
        << ",\"expected_need\":" << exp.need_u64.size() << ",";
#define RBSR_METRIC(name) out << "\"" #name "\":" << m.name << ","
    RBSR_METRIC(fullA);
    RBSR_METRIC(fullB);
    RBSR_METRIC(sliceA);
    RBSR_METRIC(sliceB);
    RBSR_METRIC(have_count);
    RBSR_METRIC(need_count);
    RBSR_METRIC(repeat_reconcile);
    RBSR_METRIC(prep_total_ms);
    RBSR_METRIC(prep_open_ms);
    RBSR_METRIC(prep_populate_ms);
    RBSR_METRIC(prep_commit_ms);
    RBSR_METRIC(prep_expected_ms);
    RBSR_METRIC(prep_serialize_ms);
    RBSR_METRIC(open_ms);
    RBSR_METRIC(build_ms);
    RBSR_METRIC(reconcile_ms);
    RBSR_METRIC(decode_sort_ms);
    RBSR_METRIC(total_bench_ms);
    RBSR_METRIC(msg_count);
    RBSR_METRIC(bytes_a_to_b);
    RBSR_METRIC(bytes_b_to_a);
    RBSR_METRIC(A_apparent_bytes);
    RBSR_METRIC(B_apparent_bytes);
    RBSR_METRIC(A_alloc_bytes);
    RBSR_METRIC(B_alloc_bytes);
    RBSR_METRIC(A_used_bytes_est);
    RBSR_METRIC(B_used_bytes_est);
    RBSR_METRIC(rss_kb_before);
    RBSR_METRIC(rss_kb_after);
#undef RBSR_METRIC
    out << "\"cpu_seconds\":null,\"io_read_bytes\":null,\"io_write_bytes\":null}\n";
    out.close();
    if (!out)
        throw std::runtime_error("failed to write machine result");
}
