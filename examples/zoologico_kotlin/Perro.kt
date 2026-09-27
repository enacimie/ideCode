class Perro(nombre: String, edad: Int, val cuidador: Cuidador) : Animal(nombre, edad) {
    val juguetes: MutableList<String> = mutableListOf()

    override fun sonido(): String {
        return "Guau"
    }
}
