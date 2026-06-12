use cf_pathpolicy::{exclude, Options};

fn main() {
    let names = [
        "api/openapi-spec/swagger.json",
        "api/openapi-spec/v3/apis__admissionregistration.k8s.io__v1_openapi.json",
        "pkg/api/testing/validation_test.go",
        "pkg/apis/admissionregistration/v1/zz_generated.validations.go",
        "pkg/apis/admissionregistration/v1alpha1/zz_generated.validations.go",
        "pkg/apis/admissionregistration/v1beta1/zz_generated.validations.go",
        "pkg/apis/admissionregistration/validation/validation.go",
        "pkg/generated/openapi/zz_generated.openapi.go",
        "pkg/registry/admissionregistration/validatingadmissionpolicybinding/declarative_validation_test.go",
        "staging/src/k8s.io/api/admissionregistration/v1/generated.proto",
        "staging/src/k8s.io/api/admissionregistration/v1/types.go",
        "staging/src/k8s.io/api/admissionregistration/v1alpha1/generated.proto",
        "staging/src/k8s.io/api/admissionregistration/v1alpha1/types.go",
        "staging/src/k8s.io/api/admissionregistration/v1beta1/generated.proto",
        "staging/src/k8s.io/api/admissionregistration/v1beta1/types.go",
    ];
    let opts = Options { include_vendored: false, include_generated: false, extra_excluded_prefixes: vec![] };
    let mut kept = 0;
    for n in names {
        let ex = exclude(n, None, &opts);
        println!("{n} excluded={ex}");
        if !ex { kept += 1; }
    }
    println!("KEPT = {kept}");
}
